//! AIFF / AIFF-C decoding to interleaved `f32`.
//!
//! AIFF is the other half of the pair that [`crate::wav`] started: same idea
//! (a chunked container around linear PCM), different byte order, plus one
//! feature RIFF does not have - AIFF-C names its compression in the header, so a
//! file can be big-endian PCM, little-endian PCM (`sowt`) or IEEE float without
//! changing its extension.
//!
//! This lives in-crate for the same reason WAV does: `symphonia`'s `aiff` feature
//! pulls `symphonia-format-riff` into the `extended` crate, which the offline
//! vendoring script cannot fetch (`scripts/README.md`). Writing the 300 lines
//! here means the spec's AIFF requirement is met *and* testable, instead of being
//! an "unsupported format" line in the docs.
//!
//! What is supported: AIFF and AIFF-C, PCM 8/16/24/32-bit (signed, as the format
//! defines), IEEE float 32/64-bit, any channel count, and the `NONE`, `twos`,
//! `sowt`, `fl32`/`FL32`, `fl64`/`FL64` compression types. What is rejected with
//! the codec's own name rather than guessed at: µ-law, A-law, IMA4 and the rest
//! of the AIFF-C compression zoo.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use crate::wav::{DecodeError, Decoder, Format, SampleLayout, sample_to_f32};

/// A parsed AIFF file: format plus the byte range of its samples.
#[derive(Debug, Clone)]
pub struct AiffDecoder {
    path: PathBuf,
    format: Format,
    layout: SampleLayout,
    data_offset: u64,
    data_len: u64,
    /// Byte offset of the next sample inside `data`.
    cursor: u64,
}

impl AiffDecoder {
    /// Opens and parses the header of `path`.
    ///
    /// # Errors
    ///
    /// Fails on I/O errors, non-FORM files, missing `COMM`/`SSND` chunks and
    /// compression types this decoder does not implement.
    pub fn open(path: &Path) -> Result<Self, DecodeError> {
        let io = |source: std::io::Error| DecodeError::Io {
            path: path.to_path_buf(),
            source,
        };
        let mut file = File::open(path).map_err(io)?;
        let file_len = file.metadata().map_err(io)?.len();
        Self::parse(path, &mut file, file_len)
    }

    fn parse(path: &Path, file: &mut File, file_len: u64) -> Result<Self, DecodeError> {
        let io = |source: std::io::Error| DecodeError::Io {
            path: path.to_path_buf(),
            source,
        };

        let mut header = [0_u8; 12];
        file.read_exact(&mut header).map_err(io)?;
        let form = &header[0..4];
        let kind = &header[8..12];
        if form != b"FORM" || (kind != b"AIFF" && kind != b"AIFC") {
            return Err(DecodeError::NotAiff {
                path: path.to_path_buf(),
            });
        }
        let is_aifc = kind == b"AIFC";

        let mut format: Option<Format> = None;
        let mut layout: Option<SampleLayout> = None;
        let mut declared_frames: Option<u64> = None;
        let mut sound: Option<(u64, u64)> = None;
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
            let size = u64::from(u32::from_be_bytes([
                chunk_header[4],
                chunk_header[5],
                chunk_header[6],
                chunk_header[7],
            ]));
            let body = position + 8;
            let available = file_len.saturating_sub(body);

            if &id == b"COMM" {
                let needed = if is_aifc { 22 } else { 18 };
                if size < needed {
                    return Err(DecodeError::Invalid("COMM chunk too short for AIFF"));
                }
                if size > available {
                    return Err(DecodeError::Truncated {
                        context: "COMM chunk",
                        wanted: size,
                        available,
                    });
                }
                let mut comm = vec![0_u8; size as usize];
                file.read_exact(&mut comm).map_err(io)?;
                let (parsed_format, parsed_layout, frames) = parse_comm(path, &comm, is_aifc)?;
                format = Some(parsed_format);
                layout = Some(parsed_layout);
                declared_frames = Some(frames);
            } else if &id == b"SSND" {
                if size < 8 {
                    return Err(DecodeError::Invalid("SSND chunk shorter than 8 bytes"));
                }
                if available < 8 {
                    return Err(DecodeError::Truncated {
                        context: "SSND chunk",
                        wanted: 8,
                        available,
                    });
                }
                let mut bounds = [0_u8; 8];
                file.read_exact(&mut bounds).map_err(io)?;
                // `offset` is a byte offset from the end of the 8-byte header to
                // the first sample; `blockSize` is only meaningful for block
                // alignment and is ignored by every player that matters.
                let offset = u64::from(u32::from_be_bytes([
                    bounds[0], bounds[1], bounds[2], bounds[3],
                ]));
                let start = body + 8 + offset;
                let len = size.saturating_sub(8).saturating_sub(offset);
                sound = Some((start.min(file_len), len.min(file_len.saturating_sub(start))));
                // Keep scanning: `COMM` may legally follow `SSND`.
            }

            // Chunks are even aligned: an odd size is followed by a pad byte.
            position = body + size + (size & 1);
        }

        let format = format.ok_or(DecodeError::MissingComm {
            path: path.to_path_buf(),
        })?;
        let layout = layout.expect("format and layout are set together");
        let (mut data_offset, mut data_len) = sound.ok_or(DecodeError::MissingSsnd {
            path: path.to_path_buf(),
        })?;

        let frame_bytes = u64::from(format.channels) * layout.bytes();
        if frame_bytes == 0 {
            return Err(DecodeError::Invalid("frame size is zero"));
        }
        // Real files sometimes declare fewer bytes than `numSampleFrames` asks
        // for (truncated downloads) and sometimes more (padding). The declared
        // frame count wins when it is smaller, and partial frames are dropped
        // either way so `read` cannot split a frame.
        if let Some(frames) = declared_frames.filter(|frames| *frames > 0) {
            data_len = data_len.min(frames * frame_bytes);
        }
        data_len = data_len / frame_bytes * frame_bytes;
        if data_offset > file_len {
            data_offset = file_len;
            data_len = 0;
        }

        Ok(Self {
            path: path.to_path_buf(),
            format,
            layout,
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

    /// Total frames, from the byte range that survived the header checks.
    #[must_use]
    pub fn frames(&self) -> u64 {
        let frame_bytes = u64::from(self.format.channels.max(1)) * self.layout.bytes().max(1);
        self.data_len / frame_bytes
    }
}

/// Reads the fields of a `COMM` chunk.
///
/// Returns the format, the sample layout, and `numSampleFrames`.
fn parse_comm(
    path: &Path,
    bytes: &[u8],
    is_aifc: bool,
) -> Result<(Format, SampleLayout, u64), DecodeError> {
    let channels = i16::from_be_bytes([bytes[0], bytes[1]]);
    let declared_frames = u64::from(u32::from_be_bytes([bytes[2], bytes[3], bytes[4], bytes[5]]));
    let sample_size = i16::from_be_bytes([bytes[6], bytes[7]]);
    let rate = parse_extended(&bytes[8..18]);
    if channels <= 0 || sample_size <= 0 {
        return Err(DecodeError::Invalid("zero channels or sample size"));
    }
    if !rate.is_finite() || rate <= 0.0 || rate > f64::from(u32::MAX) {
        return Err(DecodeError::Invalid("sample rate is not a finite number"));
    }
    let sample_rate = rate.round() as u32;

    // AIFF-C names its compression type. Plain AIFF is always big-endian PCM.
    let compression: [u8; 4] = if is_aifc {
        [bytes[18], bytes[19], bytes[20], bytes[21]]
    } else {
        *b"NONE"
    };
    let (is_float, big_endian, bits) = match &compression {
        b"NONE" | b"twos" => (false, true, sample_size as u16),
        b"sowt" => (false, false, sample_size as u16),
        b"fl32" | b"FL32" => (true, true, 32),
        b"fl64" | b"FL64" => (true, true, 64),
        other => {
            return Err(DecodeError::Unsupported {
                path: path.to_path_buf(),
                detail: format!(
                    "AIFF-C compression '{}' is not implemented",
                    String::from_utf8_lossy(other).trim()
                ),
            });
        }
    };
    let supported = if is_float {
        matches!(bits, 32 | 64)
    } else {
        matches!(bits, 8 | 16 | 24 | 32)
    };
    if !supported {
        return Err(DecodeError::Unsupported {
            path: path.to_path_buf(),
            detail: format!("{bits}-bit samples"),
        });
    }

    let format = Format {
        sample_rate,
        channels: channels as u16,
        bits_per_sample: bits,
        is_float,
    };
    Ok((
        format,
        SampleLayout::aiff(format, big_endian),
        declared_frames,
    ))
}

/// Decodes the 80-bit IEEE 754 extended float AIFF uses for its sample rate.
///
/// The format predates `f64` in the header, so the exponent is not biased the
/// way anyone expects: 16383 rather than 1023, and the mantissa is an *explicit*
/// integer bit with no hidden leading one.
fn parse_extended(bytes: &[u8]) -> f64 {
    let negative = bytes[0] & 0x80 != 0;
    let exponent = (u16::from(bytes[0] & 0x7f) << 8) | u16::from(bytes[1]);
    let mantissa = u64::from_be_bytes([
        bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7], bytes[8], bytes[9],
    ]);
    let sign = if negative { -1.0 } else { 1.0 };
    if exponent == 0 && mantissa == 0 {
        return 0.0 * sign;
    }
    if exponent == 0x7fff {
        return sign * f64::INFINITY;
    }
    sign * (mantissa as f64) * 2_f64.powi(i32::from(exponent) - 16_383 - 63)
}

impl AiffDecoder {
    /// Byte offset of the first sample of `frame`, clamped to the sound data.
    fn frame_offset(&self, frame: u64) -> u64 {
        frame.min(self.frames()) * u64::from(self.format.channels.max(1)) * self.layout.bytes()
    }
}

impl Decoder for AiffDecoder {
    fn format(&self) -> Format {
        self.format
    }

    fn total_samples(&self) -> Option<u64> {
        Some(self.data_len / self.layout.bytes().max(1))
    }

    fn read(&mut self, out: &mut [f32]) -> Result<usize, DecodeError> {
        if out.is_empty() || self.cursor >= self.data_len {
            return Ok(0);
        }
        let bytes_per_sample = self.layout.bytes().max(1);
        let remaining_samples = (self.data_len - self.cursor) / bytes_per_sample;
        let wanted = (out.len() as u64).min(remaining_samples) as usize;
        if wanted == 0 {
            return Ok(0);
        }
        let byte_count = wanted as u64 * bytes_per_sample;

        let io = |source: std::io::Error| DecodeError::Io {
            path: self.path.clone(),
            source,
        };
        let mut file = File::open(&self.path).map_err(io)?;
        file.seek(SeekFrom::Start(self.data_offset + self.cursor))
            .map_err(io)?;
        let mut raw = vec![0_u8; byte_count as usize];
        file.read_exact(&mut raw).map_err(io)?;

        for (index, sample) in raw.chunks_exact(bytes_per_sample as usize).enumerate() {
            out[index] = sample_to_f32(sample, self.layout);
        }
        self.cursor += byte_count;
        Ok(wanted)
    }

    fn seek(&mut self, position: std::time::Duration) -> Result<u64, DecodeError> {
        let rate = u64::from(self.format.sample_rate.max(1));
        let frame = position.as_micros().saturating_mul(u128::from(rate)) / 1_000_000;
        let frame = u64::try_from(frame).unwrap_or(u64::MAX);
        if frame > self.frames() {
            return Err(DecodeError::SeekOutOfRange {
                path: self.path.clone(),
                position,
            });
        }
        self.cursor = self.frame_offset(frame);
        Ok(frame)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Encodes a sample rate the way AIFF does: 80-bit extended float.
    fn extended(rate: u32) -> [u8; 10] {
        let mut out = [0_u8; 10];
        let mut value = f64::from(rate);
        // Normalise into [1, 2) and keep the exponent.
        let mut exponent = 16_383_i32;
        while value >= 2.0 {
            value /= 2.0;
            exponent += 1;
        }
        while value < 1.0 && value > 0.0 {
            value *= 2.0;
            exponent -= 1;
        }
        let mantissa = (value * 2_f64.powi(63)) as u64;
        out[0] = ((exponent >> 8) & 0x7f) as u8;
        out[1] = (exponent & 0xff) as u8;
        out[2..10].copy_from_slice(&mantissa.to_be_bytes());
        out
    }

    fn chunk(id: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(id);
        out.extend_from_slice(&(body.len() as u32).to_be_bytes());
        out.extend_from_slice(body);
        if body.len() % 2 == 1 {
            out.push(0);
        }
        out
    }

    fn comm(channels: i16, frames: u32, bits: i16, rate: u32, aifc: Option<&[u8; 4]>) -> Vec<u8> {
        let mut body = Vec::new();
        body.extend_from_slice(&channels.to_be_bytes());
        body.extend_from_slice(&frames.to_be_bytes());
        body.extend_from_slice(&bits.to_be_bytes());
        body.extend_from_slice(&extended(rate));
        if let Some(kind) = aifc {
            body.extend_from_slice(kind);
            // A one-byte compression name plus its length byte.
            body.extend_from_slice(&[1, b'N']);
        }
        chunk(b"COMM", &body)
    }

    fn sound(data: &[u8], offset: u32) -> Vec<u8> {
        let mut body = Vec::new();
        body.extend_from_slice(&offset.to_be_bytes());
        body.extend_from_slice(&0_u32.to_be_bytes());
        body.extend_from_slice(&vec![0xAA; offset as usize]);
        body.extend_from_slice(data);
        chunk(b"SSND", &body)
    }

    fn aiff_bytes(kind: &[u8; 4], chunks: Vec<u8>) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"FORM");
        out.extend_from_slice(&((chunks.len() + 4) as u32).to_be_bytes());
        out.extend_from_slice(kind);
        out.extend_from_slice(&chunks);
        out
    }

    fn write_temp(name: &str, bytes: &[u8]) -> PathBuf {
        let path = std::env::temp_dir().join(format!("aurora-aiff-{}-{name}", std::process::id()));
        let mut file = File::create(&path).expect("create temp file");
        file.write_all(bytes).expect("write temp file");
        path
    }

    fn decode_format(path: &Path) -> Format {
        AiffDecoder::open(path).expect("open").format()
    }

    fn decode_all(path: &Path) -> Vec<f32> {
        let mut decoder = AiffDecoder::open(path).expect("open");
        let mut all = Vec::new();
        let mut block = vec![0.0_f32; 256];
        loop {
            let read = decoder.read(&mut block).expect("read");
            if read == 0 {
                break;
            }
            all.extend_from_slice(&block[..read]);
        }
        all
    }

    #[test]
    fn parses_the_rate_encodings_every_other_aiff_file_uses() {
        for rate in [8_000_u32, 22_050, 44_100, 48_000, 96_000, 192_000] {
            let bytes = extended(rate);
            assert_eq!(
                parse_extended(&bytes).round() as u32,
                rate,
                "round trip for {rate}"
            );
        }
        // The exact bytes a 44.1 kHz AIFF header carries.
        assert_eq!(
            parse_extended(&[0x40, 0x0E, 0xAC, 0x44, 0, 0, 0, 0, 0, 0]),
            44_100.0
        );
        assert_eq!(
            parse_extended(&[0x40, 0x0E, 0xBB, 0x80, 0, 0, 0, 0, 0, 0]),
            48_000.0
        );
    }

    #[test]
    fn decodes_16_bit_big_endian_pcm() {
        // 1.0, -1.0, 0.5, silence: the four cases the sign extension can get wrong.
        let mut data = Vec::new();
        for value in [i16::MAX, i16::MIN + 1, 16_384, 0] {
            data.extend_from_slice(&value.to_be_bytes());
        }
        let path = write_temp(
            "16bit.aiff",
            &aiff_bytes(
                b"AIFF",
                [comm(1, 4, 16, 48_000, None), sound(&data, 0)].concat(),
            ),
        );
        let samples = decode_all(&path);
        assert_eq!(samples.len(), 4);
        assert!((samples[0] - 0.999_969_5).abs() < 1e-6, "{}", samples[0]);
        assert!((samples[1] + 0.999_969_5).abs() < 1e-6, "{}", samples[1]);
        assert!((samples[2] - 0.5).abs() < 1e-6, "{}", samples[2]);
        assert!(samples[3].abs() < 1e-9);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn eight_bit_aiff_is_signed_while_eight_bit_wav_is_not() {
        let path = write_temp(
            "8bit.aiff",
            &aiff_bytes(
                b"AIFF",
                [comm(1, 3, 8, 8_000, None), sound(&[0x40, 0xC0, 0x00], 0)].concat(),
            ),
        );
        let samples = decode_all(&path);
        assert!((samples[0] - 0.5).abs() < 1e-6, "{}", samples[0]);
        assert!((samples[1] + 0.5).abs() < 1e-6, "{}", samples[1]);
        assert!(samples[2].abs() < 1e-9);
        // The same bytes through the RIFF rule would be +1.0 and +0.5.
        let riff = SampleLayout::riff(Format {
            sample_rate: 8_000,
            channels: 1,
            bits_per_sample: 8,
            is_float: false,
        });
        assert!((sample_to_f32(&[0x40], riff) - (64.0 / 128.0 - 1.0)).abs() < 1e-6);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn decodes_24_bit_and_float_samples() {
        let mut wide = Vec::new();
        for value in [8_388_607_i32, -8_388_608, 4_194_304] {
            wide.extend_from_slice(&value.to_be_bytes()[1..4]);
        }
        let path = write_temp(
            "24bit.aiff",
            &aiff_bytes(
                b"AIFF",
                [comm(1, 3, 24, 48_000, None), sound(&wide, 0)].concat(),
            ),
        );
        let samples = decode_all(&path);
        assert!((samples[0] - 0.999_999_9).abs() < 1e-5, "{}", samples[0]);
        assert!((samples[1] + 1.0).abs() < 1e-5, "{}", samples[1]);
        assert!((samples[2] - 0.5).abs() < 1e-6, "{}", samples[2]);
        let _ = std::fs::remove_file(&path);

        let mut floats = Vec::new();
        for value in [0.25_f32, -0.5] {
            floats.extend_from_slice(&value.to_be_bytes());
        }
        let path = write_temp(
            "fl32.aifc",
            &aiff_bytes(
                b"AIFC",
                [comm(1, 2, 32, 48_000, Some(b"fl32")), sound(&floats, 0)].concat(),
            ),
        );
        let samples = decode_all(&path);
        assert!((samples[0] - 0.25).abs() < 1e-7, "{}", samples[0]);
        assert!((samples[1] + 0.5).abs() < 1e-7, "{}", samples[1]);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn aifc_sowt_reads_little_endian_pcm() {
        let mut data = Vec::new();
        for value in [16_384_i16, -16_384] {
            data.extend_from_slice(&value.to_le_bytes());
        }
        let path = write_temp(
            "sowt.aifc",
            &aiff_bytes(
                b"AIFC",
                [comm(1, 2, 16, 44_100, Some(b"sowt")), sound(&data, 0)].concat(),
            ),
        );
        let decoder = AiffDecoder::open(&path).expect("open sowt");
        assert_eq!(decoder.format().sample_rate, 44_100);
        assert_eq!(decoder.format().bits_per_sample, 16);
        let samples = decode_all(&path);
        assert!((samples[0] - 0.5).abs() < 1e-6, "{}", samples[0]);
        assert!((samples[1] + 0.5).abs() < 1e-6, "{}", samples[1]);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn the_ssnd_offset_and_odd_chunk_padding_are_honoured() {
        let mut data = Vec::new();
        for value in [16_384_i16, 0] {
            data.extend_from_slice(&value.to_be_bytes());
        }
        let path = write_temp(
            "offset.aiff",
            &aiff_bytes(
                b"AIFF",
                [
                    // An odd-sized chunk forces the parser to skip a pad byte.
                    chunk(b"ANNO", b"odd"),
                    comm(1, 2, 16, 48_000, None),
                    // Four bytes of junk between the header and the samples.
                    sound(&data, 4),
                ]
                .concat(),
            ),
        );
        let samples = decode_all(&path);
        assert_eq!(samples.len(), 2);
        assert!((samples[0] - 0.5).abs() < 1e-6, "{}", samples[0]);
        assert!(samples[1].abs() < 1e-9);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_declared_frame_count_shorter_than_the_data_wins() {
        // COMM claims two frames, SSND holds four.
        let mut data = Vec::new();
        for value in [16_384_i16, 16_384, 16_384, 16_384] {
            data.extend_from_slice(&value.to_be_bytes());
        }
        let path = write_temp(
            "trim.aiff",
            &aiff_bytes(
                b"AIFF",
                [comm(1, 2, 16, 48_000, None), sound(&data, 0)].concat(),
            ),
        );
        let decoder = AiffDecoder::open(&path).expect("open");
        assert_eq!(decoder.total_frames(), Some(2));
        let samples = decode_all(&path);
        assert_eq!(samples.len(), 2);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn seeking_lands_on_the_requested_frame() {
        let frames = 4_800_u32;
        let mut data = Vec::with_capacity(frames as usize * 2);
        for frame in 0..frames {
            data.extend_from_slice(&(frame as i16).to_be_bytes());
        }
        let path = write_temp(
            "seek.aiff",
            &aiff_bytes(
                b"AIFF",
                [comm(1, frames, 16, 48_000, None), sound(&data, 0)].concat(),
            ),
        );
        let mut decoder = AiffDecoder::open(&path).expect("open");
        // 100 ms at 48 kHz = frame 4800, which is the end of the stream.
        assert_eq!(
            decoder
                .seek(std::time::Duration::from_millis(100))
                .expect("seek to end"),
            u64::from(frames)
        );
        // 10 ms = frame 480 exactly; AIFF is PCM, so there is nothing to trim.
        let landed = decoder
            .seek(std::time::Duration::from_millis(10))
            .expect("seek");
        assert_eq!(landed, 480);
        let mut buffer = [0.0_f32; 1];
        decoder.read(&mut buffer).expect("read after seek");
        assert!((buffer[0] - 480.0 / 32_768.0).abs() < 1e-9, "{}", buffer[0]);
        assert!(matches!(
            decoder.seek(std::time::Duration::from_secs(5)),
            Err(DecodeError::SeekOutOfRange { .. })
        ));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_file_that_is_not_aiff_is_rejected_by_name() {
        let path = write_temp("plain.aiff", b"RIFF\0\0\0\0WAVEfmt ");
        let error = AiffDecoder::open(&path).expect_err("must fail");
        assert!(matches!(error, DecodeError::NotAiff { .. }), "{error}");
        assert!(error.to_string().contains("plain.aiff"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn missing_chunks_are_named() {
        let no_comm = write_temp("nocomm.aiff", &aiff_bytes(b"AIFF", sound(&[0, 0], 0)));
        assert!(matches!(
            AiffDecoder::open(&no_comm),
            Err(DecodeError::MissingComm { .. })
        ));
        let _ = std::fs::remove_file(&no_comm);

        let no_ssnd = write_temp(
            "nossnd.aiff",
            &aiff_bytes(b"AIFF", comm(1, 0, 16, 48_000, None)),
        );
        assert!(matches!(
            AiffDecoder::open(&no_ssnd),
            Err(DecodeError::MissingSsnd { .. })
        ));
        let _ = std::fs::remove_file(&no_ssnd);
    }

    #[test]
    fn an_unimplemented_compression_names_the_codec() {
        let path = write_temp(
            "ulaw.aifc",
            &aiff_bytes(
                b"AIFC",
                [comm(1, 2, 16, 8_000, Some(b"ulaw")), sound(&[0, 0], 0)].concat(),
            ),
        );
        let error = AiffDecoder::open(&path).expect_err("must fail");
        match &error {
            DecodeError::Unsupported { detail, .. } => {
                assert!(detail.contains("ulaw"), "{detail}");
            }
            other => panic!("expected Unsupported, got {other}"),
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn truncation_is_reported_rather_than_read_past_the_end() {
        // SSND declares more bytes than the file holds: the decoder clamps to
        // what is really there instead of trusting the header.
        let mut bytes = aiff_bytes(b"AIFF", comm(1, 4, 16, 48_000, None));
        let mut ssnd = Vec::new();
        ssnd.extend_from_slice(b"SSND");
        ssnd.extend_from_slice(&64_u32.to_be_bytes());
        ssnd.extend_from_slice(&[0_u8; 8]);
        ssnd.extend_from_slice(&[0x40, 0x00, 0x40, 0x00]);
        bytes.extend_from_slice(&ssnd);
        let path = write_temp("short.aiff", &bytes);
        let decoder = AiffDecoder::open(&path).expect("open truncated");
        assert_eq!(decoder.total_frames(), Some(2));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn the_committed_aiff_fixture_decodes_exactly_like_the_wav_fixture() {
        let music = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/music");
        let aiff = decode_all(&music.join("00 - Fixture Tone.aiff"));

        // The WAV side goes through the other decoder, on purpose: this asserts
        // that the shared `sample_to_f32` scaling and the two chunk walks agree.
        let mut wav_decoder =
            crate::wav::WavDecoder::open(&music.join("00 - Fixture Tone.wav")).expect("open wav");
        assert_eq!(
            wav_decoder.format(),
            decode_format(&music.join("00 - Fixture Tone.aiff"))
        );
        let mut wav = Vec::new();
        let mut block = [0.0_f32; 256];
        loop {
            let read = <crate::wav::WavDecoder as Decoder>::read(&mut wav_decoder, &mut block)
                .expect("read wav");
            if read == 0 {
                break;
            }
            wav.extend_from_slice(&block[..read]);
        }
        assert_eq!(aiff.len(), 2_400, "0.05 s at 48 kHz");
        assert_eq!(aiff.len(), wav.len());
        for (index, (left, right)) in aiff.iter().zip(wav.iter()).enumerate() {
            assert!(
                (left - right).abs() < 1e-7,
                "sample {index}: aiff {left} vs wav {right}"
            );
        }
    }
}
