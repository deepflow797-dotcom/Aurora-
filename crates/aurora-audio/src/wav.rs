//! WAV (RIFF/WAVE) decoding to interleaved `f32`.
//!
//! This is the first real decoder in the project, and it exists for three
//! reasons that have nothing to do with WAV being trendy:
//!
//! * it needs **no dependency**, so the decoder seam can be built and tested in
//!   a sandbox that cannot fetch crates;
//! * WAV/AIFF are the mastering formats this app is expected to play, and they
//!   are the only ones where decoding is small enough to be provably correct by
//!   inspection - the interesting work is the chunk walk, not entropy coding;
//! * it fixes the [`Decoder`] contract that `symphonia` will implement next
//!   (FLAC/ALAC/MP3/AAC/Opus/Vorbis), so adding those does not disturb the
//!   playback path.
//!
//! Support: PCM 8/16/24/32-bit integer, 32/64-bit IEEE float, any channel count,
//! plus `WAVE_FORMAT_EXTENSIBLE` (which is what files written by most DAWs use).
//! Not supported, and reported as such instead of guessed at: ADPCM, µ-law,
//! compressed WAV and RF64 (files > 4 GiB).

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// Everything the playback path needs to know about a decoded file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Format {
    pub sample_rate: u32,
    pub channels: u16,
    pub bits_per_sample: u16,
    /// `true` for IEEE float samples, `false` for integer PCM.
    pub is_float: bool,
}

impl Format {
    /// Frames (one sample per channel) in `total_samples` interleaved samples.
    #[must_use]
    pub fn frames(&self, total_samples: u64) -> u64 {
        total_samples / u64::from(self.channels.max(1))
    }

    /// Playing time of `frames` frames at this rate.
    #[must_use]
    pub fn duration(&self, frames: u64) -> std::time::Duration {
        std::time::Duration::from_micros(
            frames.saturating_mul(1_000_000) / u64::from(self.sample_rate.max(1)),
        )
    }
}

/// A decoder turns bytes into interleaved `f32` samples.
///
/// Implementations must be able to report their format before producing any
/// sample, because the output device is opened from it.
pub trait Decoder {
    /// Format of the stream.
    fn format(&self) -> Format;

    /// Total interleaved samples, if known up front (WAV, FLAC) - `None` for
    /// streamed formats.
    fn total_samples(&self) -> Option<u64>;

    /// Fills `out` with interleaved samples and returns how many were written.
    /// A short or empty result means end of stream.
    ///
    /// # Errors
    ///
    /// Returns [`DecodeError`] when the stream is malformed.
    fn read(&mut self, out: &mut [f32]) -> Result<usize, DecodeError>;

    /// Total frames, derived from [`Self::total_samples`].
    fn total_frames(&self) -> Option<u64> {
        self.total_samples()
            .map(|samples| self.format().frames(samples))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DecodeError {
    #[error("cannot read {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{path} is not a RIFF/WAVE file")]
    NotWav { path: PathBuf },
    #[error("{path} has no fmt chunk")]
    MissingFmt { path: PathBuf },
    #[error("{path} has no data chunk")]
    MissingData { path: PathBuf },
    #[error("unsupported WAV encoding {format:#06x} with {bits} bits per sample")]
    UnsupportedFormat { format: u16, bits: u16 },
    #[error("{context} is truncated: wanted {wanted} bytes, found {available}")]
    Truncated {
        context: &'static str,
        wanted: u64,
        available: u64,
    },
    #[error("invalid WAV data: {0}")]
    Invalid(&'static str),
    #[error("{path} contains no audio track")]
    NoAudioTrack { path: PathBuf },
    #[error("{path} does not describe its sample rate or channels")]
    MissingFormatInfo { path: PathBuf },
    #[error("{path} uses a format this build cannot decode: {detail}")]
    Unsupported { path: PathBuf, detail: String },
    #[error("cannot decode {path}: {detail}")]
    Symphonia { path: PathBuf, detail: String },
    #[error("{path} has an extension no decoder here handles")]
    UnknownExtension { path: PathBuf },
}

/// A parsed WAV file: format plus the byte range of its samples.
#[derive(Debug, Clone)]
pub struct WavDecoder {
    path: PathBuf,
    format: Format,
    data_offset: u64,
    data_len: u64,
    /// Byte offset of the next sample inside `data`.
    cursor: u64,
}

impl WavDecoder {
    /// Opens and parses the header of `path`.
    ///
    /// # Errors
    ///
    /// Fails on I/O errors, non-RIFF files, missing chunks and encodings this
    /// decoder does not implement.
    pub fn open(path: &Path) -> Result<Self, DecodeError> {
        let mut file = File::open(path).map_err(|source| DecodeError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let file_len = file
            .metadata()
            .map_err(|source| DecodeError::Io {
                path: path.to_path_buf(),
                source,
            })?
            .len();
        Self::parse(path, &mut file, file_len)
    }

    fn parse(path: &Path, file: &mut File, file_len: u64) -> Result<Self, DecodeError> {
        let io = |source: std::io::Error| DecodeError::Io {
            path: path.to_path_buf(),
            source,
        };

        let mut header = [0_u8; 12];
        file.read_exact(&mut header).map_err(io)?;
        if &header[0..4] != b"RIFF" || &header[8..12] != b"WAVE" {
            return Err(DecodeError::NotWav {
                path: path.to_path_buf(),
            });
        }

        let mut format: Option<Format> = None;
        let mut data: Option<(u64, u64)> = None;
        let mut position = 12_u64;

        while position + 8 <= file_len {
            file.seek(SeekFrom::Start(position)).map_err(io)?;
            let mut chunk_header = [0_u8; 8];
            file.read_exact(&mut chunk_header).map_err(io)?;
            let id = [
                chunk_header[0],
                chunk_header[1],
                chunk_header[2],
                chunk_header[3],
            ];
            let size = u64::from(u32::from_le_bytes([
                chunk_header[4],
                chunk_header[5],
                chunk_header[6],
                chunk_header[7],
            ]));
            let body = position + 8;

            if &id == b"fmt " {
                if size < 16 {
                    return Err(DecodeError::Invalid("fmt chunk shorter than 16 bytes"));
                }
                if body + size > file_len {
                    return Err(DecodeError::Truncated {
                        context: "fmt chunk",
                        wanted: size,
                        available: file_len.saturating_sub(body),
                    });
                }
                let mut fmt = vec![0_u8; size as usize];
                file.read_exact(&mut fmt).map_err(io)?;
                format = Some(parse_fmt(&fmt)?);
            } else if &id == b"data" {
                // Some writers put 0 or 0xFFFFFFFF in the size field (streamed
                // WAV); fall back to "rest of the file" in that case.
                let declared = if size == 0 || size == u64::from(u32::MAX) {
                    file_len.saturating_sub(body)
                } else {
                    size.min(file_len.saturating_sub(body))
                };
                data = Some((body, declared));
                // Keep scanning: `fmt ` may legally follow `data`. The first
                // data chunk is the one used, which is what every player does.
            }

            // Chunks are word aligned: an odd size is followed by a pad byte.
            position = body + size + (size & 1);
        }

        let format = format.ok_or(DecodeError::MissingFmt {
            path: path.to_path_buf(),
        })?;
        let (data_offset, data_len) = data.ok_or(DecodeError::MissingData {
            path: path.to_path_buf(),
        })?;

        let frame_bytes = u64::from(format.channels) * u64::from(format.bits_per_sample) / 8;
        if frame_bytes == 0 {
            return Err(DecodeError::Invalid("frame size is zero"));
        }
        let data_len = data_len / frame_bytes * frame_bytes;

        Ok(Self {
            path: path.to_path_buf(),
            format,
            data_offset,
            data_len,
            cursor: 0,
        })
    }

    /// Path this decoder was opened from.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

fn parse_fmt(bytes: &[u8]) -> Result<Format, DecodeError> {
    let u16_at = |offset: usize| u16::from_le_bytes([bytes[offset], bytes[offset + 1]]);
    let u32_at = |offset: usize| {
        u32::from_le_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ])
    };

    let mut tag = u16_at(0);
    let channels = u16_at(2);
    let sample_rate = u32_at(4);
    let bits_per_sample = u16_at(14);
    if channels == 0 || sample_rate == 0 {
        return Err(DecodeError::Invalid("zero channels or sample rate"));
    }

    // WAVE_FORMAT_EXTENSIBLE stores the real format in the first two bytes of
    // the sub-format GUID that follows the 16-byte base structure.
    if tag == 0xFFFE {
        if bytes.len() < 26 {
            return Err(DecodeError::Invalid("extensible fmt chunk too short"));
        }
        tag = u16_at(24);
    }

    let (is_float, supported_bits) = match tag {
        1 => (false, matches!(bits_per_sample, 8 | 16 | 24 | 32)),
        3 => (true, matches!(bits_per_sample, 32 | 64)),
        other => {
            return Err(DecodeError::UnsupportedFormat {
                format: other,
                bits: bits_per_sample,
            });
        }
    };
    if !supported_bits {
        return Err(DecodeError::UnsupportedFormat {
            format: tag,
            bits: bits_per_sample,
        });
    }

    Ok(Format {
        sample_rate,
        channels,
        bits_per_sample,
        is_float,
    })
}

impl Decoder for WavDecoder {
    fn format(&self) -> Format {
        self.format
    }

    fn total_samples(&self) -> Option<u64> {
        Some(self.data_len / (u64::from(self.format.bits_per_sample) / 8))
    }

    fn read(&mut self, out: &mut [f32]) -> Result<usize, DecodeError> {
        if out.is_empty() || self.cursor >= self.data_len {
            return Ok(0);
        }
        let bytes_per_sample = u64::from(self.format.bits_per_sample) / 8;
        let remaining_samples = (self.data_len - self.cursor) / bytes_per_sample;
        let wanted = (out.len() as u64).min(remaining_samples) as usize;
        if wanted == 0 {
            return Ok(0);
        }
        let byte_count = wanted as u64 * bytes_per_sample;

        let mut file = File::open(&self.path).map_err(|source| DecodeError::Io {
            path: self.path.clone(),
            source,
        })?;
        file.seek(SeekFrom::Start(self.data_offset + self.cursor))
            .map_err(|source| DecodeError::Io {
                path: self.path.clone(),
                source,
            })?;
        let mut raw = vec![0_u8; byte_count as usize];
        file.read_exact(&mut raw)
            .map_err(|source| DecodeError::Io {
                path: self.path.clone(),
                source,
            })?;

        for (index, sample) in raw
            .chunks_exact(bytes_per_sample as usize)
            .enumerate()
            .take(wanted)
        {
            out[index] = to_f32(sample, self.format);
        }
        self.cursor += byte_count;
        Ok(wanted)
    }
}

fn to_f32(sample: &[u8], format: Format) -> f32 {
    match (format.is_float, format.bits_per_sample) {
        (true, 32) => f32::from_le_bytes([sample[0], sample[1], sample[2], sample[3]]),
        (true, 64) => {
            let value = f64::from_le_bytes([
                sample[0], sample[1], sample[2], sample[3], sample[4], sample[5], sample[6],
                sample[7],
            ]);
            value as f32
        }
        (false, 8) => (f32::from(sample[0]) - 128.0) / 128.0,
        (false, 16) => {
            let value = i16::from_le_bytes([sample[0], sample[1]]);
            f32::from(value) / 32_768.0
        }
        (false, 24) => {
            // Sign-extend the 24-bit little-endian value into an i32.
            let value = i32::from_le_bytes([0, sample[0], sample[1], sample[2]]) >> 8;
            value as f32 / 8_388_608.0
        }
        (false, 32) => {
            let value = i32::from_le_bytes([sample[0], sample[1], sample[2], sample[3]]);
            value as f32 / 2_147_483_648.0
        }
        // `parse_fmt` rejects anything else, so this is unreachable.
        _ => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Builds a WAV file in memory. `tag` 1 = PCM, 3 = float, 0xFFFE =
    /// extensible (with `sub_tag` as the real format).
    fn wav_bytes(
        tag: u16,
        channels: u16,
        rate: u32,
        bits: u16,
        data: &[u8],
        sub_tag: u16,
    ) -> Vec<u8> {
        let mut fmt = Vec::new();
        let block_align = channels * bits / 8;
        fmt.extend_from_slice(&tag.to_le_bytes());
        fmt.extend_from_slice(&channels.to_le_bytes());
        fmt.extend_from_slice(&rate.to_le_bytes());
        fmt.extend_from_slice(&(rate * u32::from(block_align)).to_le_bytes());
        fmt.extend_from_slice(&block_align.to_le_bytes());
        fmt.extend_from_slice(&bits.to_le_bytes());
        if tag == 0xFFFE {
            fmt.extend_from_slice(&22_u16.to_le_bytes()); // cbSize
            fmt.extend_from_slice(&bits.to_le_bytes()); // valid bits
            fmt.extend_from_slice(&0_u32.to_le_bytes()); // channel mask
            fmt.extend_from_slice(&sub_tag.to_le_bytes());
            fmt.extend_from_slice(&[0_u8; 14]); // rest of the GUID
        }

        let mut body = Vec::new();
        body.extend_from_slice(b"WAVE");
        body.extend_from_slice(b"fmt ");
        body.extend_from_slice(&(fmt.len() as u32).to_le_bytes());
        body.extend_from_slice(&fmt);
        body.extend_from_slice(b"data");
        body.extend_from_slice(&(data.len() as u32).to_le_bytes());
        body.extend_from_slice(data);
        if data.len() % 2 == 1 {
            body.push(0); // padding byte, not part of the chunk
        }

        let mut out = Vec::new();
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        out.extend_from_slice(&body);
        out
    }

    fn write_temp(name: &str, bytes: &[u8]) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("aurora-wav-{}-{name}.wav", std::process::id()));
        let mut file = File::create(&path).expect("create wav");
        file.write_all(bytes).expect("write wav");
        path
    }

    #[test]
    fn decodes_16_bit_stereo() {
        let samples: [i16; 4] = [0, 16_384, -16_384, 32_767];
        let mut raw = Vec::new();
        for sample in samples {
            raw.extend_from_slice(&sample.to_le_bytes());
        }
        let path = write_temp("s16", &wav_bytes(1, 2, 44_100, 16, &raw, 1));

        let mut decoder = WavDecoder::open(&path).expect("open");
        assert_eq!(
            decoder.format(),
            Format {
                sample_rate: 44_100,
                channels: 2,
                bits_per_sample: 16,
                is_float: false
            }
        );
        assert_eq!(decoder.total_frames(), Some(2));
        assert_eq!(
            decoder.format().duration(2),
            std::time::Duration::from_micros(45)
        );

        let mut out = [0.0_f32; 4];
        assert_eq!(decoder.read(&mut out).expect("read"), 4);
        assert!((out[0] - 0.0).abs() < 1e-6);
        assert!((out[1] - 0.5).abs() < 1e-6);
        assert!((out[2] + 0.5).abs() < 1e-6);
        assert!((out[3] - 0.999_969_5).abs() < 1e-4);
        assert_eq!(decoder.read(&mut out).expect("eof"), 0);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn decodes_24_bit_and_32_bit_float() {
        // 24-bit: -1.0, 0.5, 0.0 (mono)
        let mut raw24 = Vec::new();
        for value in [-8_388_608_i32, 4_194_304, 0] {
            let bytes = value.to_le_bytes();
            raw24.extend_from_slice(&bytes[0..3]);
        }
        let path = write_temp("s24", &wav_bytes(1, 1, 48_000, 24, &raw24, 1));
        let mut decoder = WavDecoder::open(&path).expect("open 24");
        let mut out = [0.0_f32; 3];
        assert_eq!(decoder.read(&mut out).expect("read"), 3);
        assert!((out[0] + 1.0).abs() < 1e-6);
        assert!((out[1] - 0.5).abs() < 1e-6);
        assert!(out[2].abs() < 1e-6);
        std::fs::remove_file(&path).ok();

        let mut raw32 = Vec::new();
        for value in [0.25_f32, -0.75] {
            raw32.extend_from_slice(&value.to_le_bytes());
        }
        let path = write_temp("f32", &wav_bytes(3, 1, 96_000, 32, &raw32, 3));
        let mut decoder = WavDecoder::open(&path).expect("open f32");
        assert!(decoder.format().is_float);
        assert_eq!(decoder.format().sample_rate, 96_000);
        let mut out = [0.0_f32; 2];
        assert_eq!(decoder.read(&mut out).expect("read"), 2);
        assert!((out[0] - 0.25).abs() < 1e-6);
        assert!((out[1] + 0.75).abs() < 1e-6);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn extensible_headers_use_the_sub_format() {
        let raw = [0_u8; 4]; // one silent 16-bit stereo frame
        let path = write_temp("ext", &wav_bytes(0xFFFE, 2, 44_100, 16, &raw, 1));
        let decoder = WavDecoder::open(&path).expect("open extensible");
        assert_eq!(decoder.format().channels, 2);
        assert!(!decoder.format().is_float);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn odd_chunk_sizes_are_word_aligned() {
        // 8-bit mono, 3 samples (odd) - the data chunk gets a pad byte.
        let path = write_temp("odd", &wav_bytes(1, 1, 8_000, 8, &[130, 127, 129], 1));
        let mut decoder = WavDecoder::open(&path).expect("open odd");
        let mut out = [0.0_f32; 3];
        assert_eq!(decoder.read(&mut out).expect("read"), 3);
        assert!((out[0] - (130.0 - 128.0) / 128.0).abs() < 1e-6);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn a_non_riff_file_is_rejected() {
        let path = write_temp("bad", b"not a wav file at all");
        let err = WavDecoder::open(&path).expect_err("must fail");
        assert!(matches!(err, DecodeError::NotWav { .. }), "{err}");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn unsupported_encodings_name_the_format() {
        let path = write_temp("adpcm", &wav_bytes(2, 1, 8_000, 4, &[0; 4], 2));
        let err = WavDecoder::open(&path).expect_err("must fail");
        match err {
            DecodeError::UnsupportedFormat { format, bits } => {
                assert_eq!((format, bits), (2, 4));
            }
            other => panic!("wrong error: {other}"),
        }
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn a_missing_file_reports_the_path() {
        let missing = std::env::temp_dir().join("aurora-wav-does-not-exist.wav");
        let err = WavDecoder::open(&missing).expect_err("must fail");
        assert!(matches!(err, DecodeError::Io { .. }), "{err}");
    }

    #[test]
    fn partial_reads_walk_through_the_stream() {
        let mut raw = Vec::new();
        for value in [1_i16, 2, 3, 4, 5, 6] {
            raw.extend_from_slice(&value.to_le_bytes());
        }
        let path = write_temp("partial", &wav_bytes(1, 2, 8_000, 16, &raw, 1));
        let mut decoder = WavDecoder::open(&path).expect("open");
        let mut out = [0.0_f32; 2];
        assert_eq!(decoder.read(&mut out).expect("first"), 2);
        assert!(out[0] > 0.0);
        assert_eq!(decoder.read(&mut out).expect("second"), 2);
        assert_eq!(decoder.read(&mut out).expect("third"), 2);
        assert_eq!(decoder.read(&mut out).expect("eof"), 0);
        std::fs::remove_file(&path).ok();
    }
}
