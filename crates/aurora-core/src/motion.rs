//! Motion primitives shared by every UI target.
//!
//! The animation *math* lives in core (not in the UI crate) so the Linux build,
//! the Android build and the headless spike benchmark animate identically - a
//! frame-time number measured in the spike means something for both targets.

use std::time::Duration;

/// Critically-damped-ish spring used for the lyric scroll position.
///
/// Semi-implicit Euler with substepping: stable at any frame time, including the
/// 16.7 ms frames of the spike and the much longer ones a backgrounded phone can
/// produce when it wakes up.
#[derive(Debug, Clone, Copy)]
pub struct Spring {
    pub stiffness: f32,
    pub damping: f32,
    pub position: f32,
    pub velocity: f32,
}

impl Spring {
    #[must_use]
    pub fn new(position: f32, stiffness: f32, damping: f32) -> Self {
        Self {
            stiffness,
            damping,
            position,
            velocity: 0.0,
        }
    }

    /// A scroll spring tuned for lyric auto-follow: quick but never wobbly.
    #[must_use]
    pub fn scroll(position: f32) -> Self {
        Self::new(position, 180.0, 26.0)
    }

    /// Advances the spring by `dt`, clamped to 100 ms per call so a hitch cannot
    /// explode the integrator.
    pub fn advance(&mut self, target: f32, dt: Duration) {
        let dt = dt.as_secs_f32().min(0.1);
        let substeps = ((dt / (1.0 / 240.0)).ceil() as u32).max(1);
        let h = dt / substeps as f32;
        for _ in 0..substeps {
            let accel = -self.stiffness * (self.position - target) - self.damping * self.velocity;
            self.velocity += accel * h;
            self.position += self.velocity * h;
        }
    }

    /// True when the spring has effectively settled (used to skip redraws).
    #[must_use]
    pub fn is_settled(&self, target: f32) -> bool {
        (self.position - target).abs() < 0.05 && self.velocity.abs() < 0.05
    }
}

/// Classic ease-out used for one-shot transitions (panel slides, fades).
#[must_use]
pub fn ease_out_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

/// Envelope applied to a word as it is "sung": scale/glow ramp up over `fade`,
/// then hold; callers multiply opacity by it.
///
/// Returns `(scale, opacity)` where scale goes from `1.0` to `1.0 + amount`.
#[must_use]
pub fn word_envelope(activation: f32, amount: f32) -> (f32, f32) {
    let e = ease_out_cubic(activation);
    (1.0 + amount * e, 0.55 + 0.45 * e)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spring_converges_without_overshoot() {
        let mut spring = Spring::scroll(0.0);
        let mut max = 0.0f32;
        for _ in 0..240 {
            spring.advance(100.0, Duration::from_millis(16));
            max = max.max(spring.position);
        }
        assert!(
            (spring.position - 100.0).abs() < 0.5,
            "position {}",
            spring.position
        );
        assert!(max < 104.0, "overshoot {max}");
    }

    #[test]
    fn spring_is_stable_with_huge_dt() {
        let mut spring = Spring::scroll(0.0);
        spring.advance(500.0, Duration::from_secs(5));
        assert!(spring.position.is_finite());
    }

    #[test]
    fn envelope_is_monotonic() {
        let mut prev = 0.0;
        for i in 0..=10 {
            let (scale, opacity) = word_envelope(i as f32 / 10.0, 0.25);
            assert!(scale >= prev);
            prev = scale;
            assert!((0.5..=1.0).contains(&opacity));
        }
    }
}
