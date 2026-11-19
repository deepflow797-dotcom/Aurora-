//! Playback state machine.
//!
//! Intentionally tiny and side-effect free: commands mutate state, the app crate
//! is responsible for making the audio backend follow. Keeping it pure means the
//! same transitions can be unit tested and replayed from the Android layer.

use crate::model::TrackId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlaybackState {
    #[default]
    Stopped,
    Playing,
    Paused,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Transport {
    #[default]
    Stopped,
    Playing {
        track: TrackId,
        queue_pos: usize,
    },
    Paused {
        track: TrackId,
        queue_pos: usize,
    },
}

impl Transport {
    #[must_use]
    pub fn state(&self) -> PlaybackState {
        match self {
            Self::Stopped => PlaybackState::Stopped,
            Self::Playing { .. } => PlaybackState::Playing,
            Self::Paused { .. } => PlaybackState::Paused,
        }
    }

    #[must_use]
    pub fn current(&self) -> Option<(TrackId, usize)> {
        match self {
            Self::Stopped => None,
            Self::Playing { track, queue_pos } | Self::Paused { track, queue_pos } => {
                Some((*track, *queue_pos))
            }
        }
    }

    /// `play`/`pause` toggle; stopping is a separate command so that a
    /// paused track keeps its position.
    pub fn toggle(&mut self) {
        *self = match self.clone() {
            Self::Playing { track, queue_pos } => Self::Paused { track, queue_pos },
            Self::Paused { track, queue_pos } => Self::Playing { track, queue_pos },
            Self::Stopped => Self::Stopped,
        };
    }

    pub fn start(&mut self, track: TrackId, queue_pos: usize) {
        *self = Self::Playing { track, queue_pos };
    }

    pub fn stop(&mut self) {
        *self = Self::Stopped;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggle_round_trip() {
        let mut t = Transport::default();
        t.start(TrackId(7), 0);
        assert_eq!(t.state(), PlaybackState::Playing);
        t.toggle();
        assert_eq!(t.state(), PlaybackState::Paused);
        assert_eq!(t.current(), Some((TrackId(7), 0)));
        t.toggle();
        assert_eq!(t.state(), PlaybackState::Playing);
        t.stop();
        assert_eq!(t.state(), PlaybackState::Stopped);
        assert_eq!(t.current(), None);
    }
}
