//! Decode inspection: the `aurora --decode <FILE>` command.
//!
//! It answers the only question a user asks of a decoder before trusting it:
//! *does this file open, what is in it, and does it contain audio that is not
//! silence?* Format, duration, peak and RMS are printed; the whole file is
//! decoded in blocks, so a malformed stream fails here rather than 40 minutes
//! into playback.
//!
//! This is also the first place the real playback shape is exercised:
//! `Decoder::read` → interleaved `f32` → per-block statistics.

use std::path::Path;

use aurora_audio::wav::{DecodeError, Format};

/// What was found in a file.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioSummary {
    pub format: Format,
    pub frames: u64,
    pub duration: std::time::Duration,
    /// Highest absolute sample value, `0.0..=1.0` for full scale.
    pub peak: f32,
    /// Root-mean-square of all interleaved samples.
    pub rms: f32,
}

impl AudioSummary {
    /// Peak level in dBFS; `f32::NEG_INFINITY` for digital silence.
    #[must_use]
    pub fn peak_dbfs(&self) -> f32 {
        if self.peak > 0.0 {
            20.0 * self.peak.log10()
        } else {
            f32::NEG_INFINITY
        }
    }

    /// True when the file decoded but carries no signal at all.
    #[must_use]
    pub fn is_silent(&self) -> bool {
        self.peak == 0.0
    }
}

/// Decodes `path` in 4096-sample blocks and summarises it.
///
/// # Errors
///
/// Propagates header and stream errors from [`WavDecoder`].
pub fn inspect(path: &Path) -> Result<AudioSummary, DecodeError> {
    let mut decoder = aurora_audio::open(path)?;
    let format = decoder.format();
    let mut block = vec![0.0_f32; 4096];
    let mut frames = 0_u64;
    let mut peak = 0.0_f32;
    let mut sum_squares = 0.0_f64;
    let mut samples = 0_u64;

    loop {
        let read = decoder.read(&mut block)?;
        if read == 0 {
            break;
        }
        for sample in &block[..read] {
            peak = peak.max(sample.abs());
            sum_squares += f64::from(*sample) * f64::from(*sample);
        }
        samples += read as u64;
        frames += format.frames(read as u64);
    }

    let rms = if samples == 0 {
        0.0
    } else {
        (sum_squares / samples as f64).sqrt() as f32
    };

    Ok(AudioSummary {
        format,
        frames,
        duration: format.duration(frames),
        peak,
        rms,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/music/00 - Fixture Tone.wav")
    }

    #[test]
    fn inspects_the_bundled_tone() {
        let summary = inspect(&fixture()).expect("decode fixture");
        assert_eq!(summary.format.sample_rate, 48_000);
        assert_eq!(summary.format.channels, 1);
        assert_eq!(summary.frames, 2_400, "0.05 s at 48 kHz");
        assert_eq!(summary.duration, std::time::Duration::from_millis(50));
        assert!((summary.peak - 0.25).abs() < 0.01, "peak {}", summary.peak);
        assert!(
            summary.rms > 0.15 && summary.rms < 0.2,
            "rms {}",
            summary.rms
        );
        assert!(!summary.is_silent());
        // 0.25 full scale ≈ -12 dBFS.
        assert!(
            (summary.peak_dbfs() + 12.04).abs() < 0.2,
            "{}",
            summary.peak_dbfs()
        );
    }

    #[test]
    fn the_aiff_fixture_reports_the_same_numbers_as_the_wav_one() {
        let aiff = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/music/00 - Fixture Tone.aiff");
        let summary = inspect(&aiff).expect("decode aiff");
        let wav = inspect(&fixture()).expect("decode wav");
        // Same audio, two containers and two byte orders: every number must
        // match, or one of the decoders has a scaling bug.
        assert_eq!(summary.format, wav.format);
        assert_eq!(summary.frames, 2_400);
        assert_eq!(summary.duration, std::time::Duration::from_millis(50));
        assert!((summary.peak - wav.peak).abs() < 1e-6, "{}", summary.peak);
        assert!((summary.rms - wav.rms).abs() < 1e-6, "{}", summary.rms);
        assert!((summary.peak - 0.25).abs() < 0.01, "peak {}", summary.peak);
    }

    #[test]
    fn inspects_the_flac_fixture_through_the_same_path() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/music/00 - Fixture Steps.flac");
        let summary = inspect(&path).expect("decode flac");
        assert_eq!(summary.format.sample_rate, 48_000);
        assert_eq!(summary.format.channels, 1);
        assert_eq!(summary.frames, 8_192);
        assert_eq!(summary.duration, std::time::Duration::from_micros(170_666));
        assert!((summary.peak - 0.25).abs() < 1e-6);
        // Half the file is a constant 0.25, half is silence: rms = 0.25/sqrt(2).
        assert!(
            (summary.rms - 0.176_777).abs() < 1e-4,
            "rms {}",
            summary.rms
        );
    }

    #[test]
    fn an_unknown_extension_is_named_not_guessed() {
        let err = inspect(Path::new("/tmp/track.xyz")).expect_err("must fail");
        assert!(matches!(err, DecodeError::UnknownExtension { .. }), "{err}");
    }

    #[test]
    fn a_missing_file_is_an_error_not_a_panic() {
        let err = inspect(Path::new("/nonexistent/track.wav")).expect_err("must fail");
        assert!(matches!(err, DecodeError::Io { .. }), "{err}");
    }

    #[test]
    fn digital_silence_reports_minus_infinity() {
        let summary = AudioSummary {
            format: Format {
                sample_rate: 48_000,
                channels: 2,
                bits_per_sample: 16,
                is_float: false,
            },
            frames: 0,
            duration: std::time::Duration::ZERO,
            peak: 0.0,
            rms: 0.0,
        };
        assert!(summary.is_silent());
        assert!(summary.peak_dbfs().is_infinite() && summary.peak_dbfs() < 0.0);
    }
}
