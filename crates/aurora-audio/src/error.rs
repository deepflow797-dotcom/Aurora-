//! The one error type every decoder in this crate reports.
//!
//! It started life inside [`crate::wav`] when WAV was the only decoder. It lives
//! here now because AIFF needed its own variants and a decoder module should not
//! own the vocabulary of every other decoder. `crate::wav::DecodeError` still
//! resolves, so nothing that already imported it had to change.
//!
//! Two rules shape the variants:
//!
//! * a failure names the **file** it happened on, because the UI shows it to a
//!   person who has ten thousand files and needs to know which one broke;
//! * nothing is guessed. An encoding the decoder does not implement is
//!   [`DecodeError::Unsupported`] with the codec's own name in `detail`, never
//!   silence or noise.

use std::path::PathBuf;

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
    #[error("{path} is not a FORM/AIFF file")]
    NotAiff { path: PathBuf },
    #[error("{path} has no COMM chunk")]
    MissingComm { path: PathBuf },
    #[error("{path} has no SSND chunk")]
    MissingSsnd { path: PathBuf },
    #[error("unsupported WAV encoding {format:#06x} with {bits} bits per sample")]
    UnsupportedFormat { format: u16, bits: u16 },
    #[error("{context} is truncated: wanted {wanted} bytes, found {available}")]
    Truncated {
        context: &'static str,
        wanted: u64,
        available: u64,
    },
    #[error("invalid audio data: {0}")]
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
    #[error("cannot seek {path} to {position:?}: the stream is not seekable")]
    SeekUnsupported {
        path: PathBuf,
        position: std::time::Duration,
    },
    #[error("cannot seek {path} to {position:?}: past the end of the stream")]
    SeekOutOfRange {
        path: PathBuf,
        position: std::time::Duration,
    },
}

impl DecodeError {
    /// The file the error is about, when it is about one file.
    #[must_use]
    pub fn path(&self) -> Option<&std::path::Path> {
        match self {
            Self::Io { path, .. }
            | Self::NotWav { path }
            | Self::MissingFmt { path }
            | Self::MissingData { path }
            | Self::NotAiff { path }
            | Self::MissingComm { path }
            | Self::MissingSsnd { path }
            | Self::NoAudioTrack { path }
            | Self::MissingFormatInfo { path }
            | Self::Unsupported { path, .. }
            | Self::Symphonia { path, .. }
            | Self::UnknownExtension { path }
            | Self::SeekUnsupported { path, .. }
            | Self::SeekOutOfRange { path, .. } => Some(path),
            Self::UnsupportedFormat { .. } | Self::Truncated { .. } | Self::Invalid(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn every_file_scoped_error_names_its_path() {
        let path = PathBuf::from("/music/broken.aiff");
        let errors = [
            DecodeError::NotAiff { path: path.clone() },
            DecodeError::MissingComm { path: path.clone() },
            DecodeError::MissingSsnd { path: path.clone() },
            DecodeError::SeekOutOfRange {
                path: path.clone(),
                position: Duration::from_secs(9),
            },
        ];
        for error in errors {
            assert_eq!(error.path(), Some(path.as_path()), "{error}");
            assert!(error.to_string().contains("/music/broken.aiff"), "{error}");
        }
    }

    #[test]
    fn structural_errors_do_not_pretend_to_know_a_file() {
        assert!(DecodeError::Invalid("frame size is zero").path().is_none());
        assert!(
            DecodeError::UnsupportedFormat { format: 2, bits: 4 }
                .path()
                .is_none()
        );
    }
}
