//! `symphonia`-backed decoding: FLAC, ALAC, MP3, AAC, CAF, M4A.
//!
//! `symphonia` is the decoder the technology selection picked, so this module is
//! deliberately thin: probe, build a decoder, pull packets, copy them out as
//! interleaved `f32`. Everything Aurora-specific (the [`Decoder`] contract, the
//! position semantics, error mapping) lives here; nothing about the codecs does.
//!
//! Two consequences of the offline sandbox are visible in the feature list
//! (`Cargo.toml`): WAV/AIFF are *not* handled by `symphonia-format-riff` (it
//! needs the `extended` crate, which cannot be fetched here), so WAV goes through
//! [`crate::wav`] and AIFF is not supported yet.

use std::fs::File;
use std::path::{Path, PathBuf};

use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{CODEC_TYPE_NULL, DecoderOptions};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::core::sample::SampleFormat;
use symphonia::core::units::TimeBase;

use crate::wav::{DecodeError, Decoder, Format};

/// A decoder for everything `symphonia` knows about.
pub struct SymphoniaDecoder {
    path: PathBuf,
    format: Format,
    reader: Box<dyn FormatReader>,
    decoder: Box<dyn symphonia::core::codecs::Decoder>,
    track_id: u32,
    total_frames: Option<u64>,
    /// Decoded samples not yet handed to the caller.
    pending: Vec<f32>,
    pending_at: usize,
    finished: bool,
    /// Converts a packet timestamp into samples.
    time_base: Option<TimeBase>,
    /// After a seek: the frame the caller asked for. The demuxer may hand back
    /// the packet *containing* it (or an earlier one), so the first packet is
    /// trimmed until this frame is reached.
    target_frame: Option<u64>,
}

impl SymphoniaDecoder {
    /// Opens `path` and builds the first audio track's decoder.
    ///
    /// # Errors
    ///
    /// Fails when the file cannot be read, is not a container `symphonia`
    /// recognises, has no audio track, or uses a codec that was not compiled in.
    pub fn open(path: &Path) -> Result<Self, DecodeError> {
        let io = |source: std::io::Error| DecodeError::Io {
            path: path.to_path_buf(),
            source,
        };

        let file = File::open(path).map_err(io)?;
        let stream = MediaSourceStream::new(Box::new(file), Default::default());

        let mut hint = Hint::new();
        if let Some(extension) = path.extension().and_then(|ext| ext.to_str()) {
            hint.with_extension(extension);
        }

        let probed = symphonia::default::get_probe()
            .format(
                &hint,
                stream,
                &FormatOptions {
                    // Reject a stream that changes mid-file: for a player it is a
                    // different file, not a continuation.
                    enable_gapless: true,
                    ..FormatOptions::default()
                },
                &MetadataOptions::default(),
            )
            .map_err(|err| map_symphonia(err, path))?;

        let reader = probed.format;
        let track = reader
            .default_track()
            .ok_or_else(|| DecodeError::NoAudioTrack {
                path: path.to_path_buf(),
            })?
            .clone();
        if track.codec_params.codec == CODEC_TYPE_NULL {
            return Err(DecodeError::NoAudioTrack {
                path: path.to_path_buf(),
            });
        }

        let sample_rate =
            track
                .codec_params
                .sample_rate
                .ok_or_else(|| DecodeError::MissingFormatInfo {
                    path: path.to_path_buf(),
                })?;
        let channels = track
            .codec_params
            .channels
            .map_or(0, |channels| channels.count() as u16);
        if channels == 0 {
            return Err(DecodeError::MissingFormatInfo {
                path: path.to_path_buf(),
            });
        }
        let bits_per_sample =
            u16::try_from(track.codec_params.bits_per_sample.unwrap_or(32)).unwrap_or(32);
        let is_float = matches!(
            track.codec_params.sample_format,
            Some(SampleFormat::F32 | SampleFormat::F64)
        );

        let decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &DecoderOptions::default())
            .map_err(|err| map_symphonia(err, path))?;

        Ok(Self {
            path: path.to_path_buf(),
            format: Format {
                sample_rate,
                channels,
                bits_per_sample,
                is_float,
            },
            track_id: track.id,
            total_frames: track.codec_params.n_frames,
            time_base: track.codec_params.time_base,
            reader,
            decoder,
            pending: Vec::new(),
            pending_at: 0,
            finished: false,
            target_frame: None,
        })
    }

    /// Decodes one packet into `pending`. Returns `false` at end of stream.
    fn fill(&mut self) -> Result<bool, DecodeError> {
        if self.finished {
            return Ok(false);
        }
        loop {
            let packet = match self.reader.next_packet() {
                Ok(packet) => packet,
                Err(SymphoniaError::IoError(err))
                    if err.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    self.finished = true;
                    return Ok(false);
                }
                Err(err) => return Err(map_symphonia(err, &self.path)),
            };

            // Containers can interleave several tracks; ignore the others.
            if packet.track_id() != self.track_id {
                continue;
            }

            let decoded = match self.decoder.decode(&packet) {
                Ok(decoded) => decoded,
                // A damaged packet must not stop playback: skip it and continue.
                Err(SymphoniaError::DecodeError(_)) => continue,
                Err(err) => return Err(map_symphonia(err, &self.path)),
            };

            let spec = *decoded.spec();
            let channels = spec.channels.count();
            let mut samples = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
            samples.copy_interleaved_ref(decoded);

            // Trim the packet down to the frame a seek asked for.
            let mut skip_samples = 0;
            if let Some(target) = self.target_frame {
                let packet_frame =
                    packet_frame_in_samples(packet.ts, self.format.sample_rate, self.time_base);
                let frames = samples.samples().len() / channels.max(1);
                if packet_frame.saturating_add(frames as u64) <= target {
                    // This packet is entirely before the target.
                    continue;
                }
                skip_samples = usize::try_from(target.saturating_sub(packet_frame))
                    .unwrap_or(usize::MAX)
                    .saturating_mul(channels)
                    .min(samples.samples().len());
                self.target_frame = None;
            }

            self.pending.clear();
            self.pending
                .extend_from_slice(&samples.samples()[skip_samples..]);
            self.pending_at = 0;
            return Ok(!self.pending.is_empty());
        }
    }
}

impl std::fmt::Debug for SymphoniaDecoder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SymphoniaDecoder")
            .field("path", &self.path)
            .field("format", &self.format)
            .field("track_id", &self.track_id)
            .field("total_frames", &self.total_frames)
            .field("finished", &self.finished)
            .finish_non_exhaustive()
    }
}

impl Decoder for SymphoniaDecoder {
    fn seek(&mut self, position: std::time::Duration) -> Result<u64, DecodeError> {
        let time = symphonia::core::units::Time::new(
            position.as_secs(),
            f64::from(position.subsec_nanos()) / 1e9,
        );
        let seeked = self
            .reader
            .seek(
                SeekMode::Accurate,
                SeekTo::Time {
                    time,
                    track_id: Some(self.track_id),
                },
            )
            .map_err(|err| match err {
                SymphoniaError::SeekError(_) => DecodeError::SeekUnsupported {
                    path: self.path.clone(),
                    position,
                },
                other => map_symphonia(other, &self.path),
            })?;

        // Discard whatever the decoder buffered for the old position, or the
        // first packet after the seek would replay pre-seek audio.
        self.decoder.reset();
        self.pending.clear();
        self.pending_at = 0;
        self.finished = false;

        // The demuxer lands at or before the request and reports where. The gap
        // up to the requested frame is trimmed when the next packet is decoded;
        // without that, a file with no seek table replays the containing block.
        let requested = position
            .as_micros()
            .saturating_mul(u128::from(self.format.sample_rate))
            / 1_000_000;
        self.target_frame = Some(u64::try_from(requested).unwrap_or(u64::MAX));
        Ok(seeked.required_ts)
    }

    fn format(&self) -> Format {
        self.format
    }

    fn total_samples(&self) -> Option<u64> {
        self.total_frames
            .map(|frames| frames * u64::from(self.format.channels))
    }

    fn read(&mut self, out: &mut [f32]) -> Result<usize, DecodeError> {
        let mut written = 0;
        while written < out.len() {
            if self.pending_at >= self.pending.len() && !self.fill()? {
                break;
            }
            let available = self.pending.len() - self.pending_at;
            let take = available.min(out.len() - written);
            out[written..written + take]
                .copy_from_slice(&self.pending[self.pending_at..self.pending_at + take]);
            self.pending_at += take;
            written += take;
        }
        Ok(written)
    }
}

/// Converts a packet timestamp (in the track's time base) to a frame index.
///
/// Time bases are `numer/denom` seconds per tick, so one tick is
/// `numer * sample_rate / denom` frames - the identity for the FLAC/MP3 case
/// (`1/sample_rate`).
fn packet_frame_in_samples(ts: u64, sample_rate: u32, time_base: Option<TimeBase>) -> u64 {
    match time_base {
        None => ts,
        Some(base) if base.numer == 1 && base.denom == sample_rate => ts,
        Some(base) => {
            let numerator = u128::from(ts) * u128::from(base.numer) * u128::from(sample_rate);
            u64::try_from(numerator / u128::from(base.denom.max(1))).unwrap_or(u64::MAX)
        }
    }
}

fn map_symphonia(err: SymphoniaError, path: &Path) -> DecodeError {
    match err {
        SymphoniaError::IoError(source) => DecodeError::Io {
            path: path.to_path_buf(),
            source,
        },
        SymphoniaError::Unsupported(_) => DecodeError::Unsupported {
            path: path.to_path_buf(),
            detail: err.to_string(),
        },
        other => DecodeError::Symphonia {
            path: path.to_path_buf(),
            detail: other.to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/music")
            .join(name)
    }

    #[test]
    fn decodes_the_flac_fixture() {
        let mut decoder =
            SymphoniaDecoder::open(&fixture("00 - Fixture Steps.flac")).expect("open flac");
        assert_eq!(
            decoder.format(),
            Format {
                sample_rate: 48_000,
                channels: 1,
                bits_per_sample: 16,
                is_float: false,
            }
        );
        assert_eq!(decoder.total_frames(), Some(8_192));

        let mut out = vec![0.0_f32; 8_192];
        assert_eq!(decoder.read(&mut out).expect("read"), 8_192);

        // Frame 0 is a constant 8192 (= 0.25 full scale), frame 1 is silence.
        assert!((out[0] - 0.25).abs() < 1e-6, "first sample {}", out[0]);
        assert!((out[4_095] - 0.25).abs() < 1e-6, "last of block 0");
        assert!(out[4_096].abs() < 1e-6, "first of block 1");
        assert!(out[8_191].abs() < 1e-6, "last sample");

        assert_eq!(decoder.read(&mut out).expect("eof"), 0);
    }

    #[test]
    fn seeking_into_flac_lands_on_the_right_block() {
        let mut decoder =
            SymphoniaDecoder::open(&fixture("00 - Fixture Steps.flac")).expect("open flac");

        // Block 1 starts at frame 4096 = 85.33 ms and is silent.
        let frame = decoder
            .seek(std::time::Duration::from_micros(85_333))
            .expect("seek to block 1");
        // An accurate seek lands at or *before* the request, so frame 4095 (the
        // last sample of block 0) is a correct answer for 4096/48000 s.
        assert!(frame <= 4_096, "must not overshoot, landed on {frame}");
        let mut out = [0.0_f32; 8];
        assert_eq!(decoder.read(&mut out).expect("read after seek"), 8);
        assert!(
            (out[0] - 0.25).abs() < 1e-6,
            "frame {frame} should still be block 0, got {}",
            out[0]
        );
        assert!(
            out[1..].iter().all(|sample| sample.abs() < 1e-6),
            "frames after {} must be silent, got {:?}",
            frame + 1,
            &out[1..]
        );

        // Back to the start: block 0 is a constant 0.25.
        assert_eq!(decoder.seek(std::time::Duration::ZERO).expect("rewind"), 0);
        let read = decoder.read(&mut out).expect("read after rewind");
        assert!(read > 0);
        assert!((out[0] - 0.25).abs() < 1e-6, "first sample {}", out[0]);
    }

    #[test]
    fn a_missing_file_is_reported_with_its_path() {
        let err = SymphoniaDecoder::open(Path::new("/nope/missing.flac")).expect_err("must fail");
        match err {
            DecodeError::Io { path, .. } => assert_eq!(path, Path::new("/nope/missing.flac")),
            other => panic!("wrong error: {other}"),
        }
    }

    #[test]
    fn junk_is_not_mistaken_for_audio() {
        let path = std::env::temp_dir().join(format!("aurora-junk-{}.flac", std::process::id()));
        std::fs::write(&path, b"this is not a flac stream, not even close").expect("write");
        let err = SymphoniaDecoder::open(&path).expect_err("must fail");
        assert!(
            matches!(
                err,
                DecodeError::Symphonia { .. } | DecodeError::Unsupported { .. }
            ),
            "{err}"
        );
        std::fs::remove_file(&path).ok();
    }
}
