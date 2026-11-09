//! Fixed-timestep clock for the game loop (task 8). The game simulates in fixed steps of 1/30 s,
//! exactly like the original 30 fps frame limiter, so physics, handling and script timings stay the
//! same; frames are drawn as often as the display allows, and `alpha` says how far the display is
//! between the last simulation step and the next one (for render interpolation).

use std::time::Duration;

#[derive(Clone, Debug)]
pub struct FixedStep {
    step: Duration,
    max_steps: u32,
    max_elapsed: Duration,
    acc: Duration,
    started: bool,
}

impl FixedStep {
    /// `hz` simulation steps per second; at most `max_steps` per frame (the rest of a long stall is
    /// dropped, so the game slows down briefly instead of spiralling).
    pub fn new(hz: u32, max_steps: u32) -> Self {
        FixedStep {
            step: Duration::from_secs(1) / hz,
            max_steps,
            max_elapsed: Duration::from_millis(250),
            acc: Duration::ZERO,
            started: false,
        }
    }

    pub fn step(&self) -> Duration {
        self.step
    }

    /// Adds the real time since the previous frame; returns (steps to simulate now, alpha 0..1).
    /// The very first call always yields one step, so a frame is simulated before it's drawn.
    pub fn advance(&mut self, elapsed: Duration) -> (u32, f32) {
        if !self.started {
            self.started = true;
            self.acc = self.step;
        } else {
            self.acc += elapsed.min(self.max_elapsed);
        }
        let mut steps = 0;
        while self.acc >= self.step && steps < self.max_steps {
            self.acc -= self.step;
            steps += 1;
        }
        if self.acc >= self.step {
            // Couldn't catch up: drop whole steps but keep the fraction for smooth alpha.
            self.acc = Duration::from_nanos((self.acc.as_nanos() % self.step.as_nanos()) as u64);
        }
        (steps, self.acc.as_secs_f32() / self.step.as_secs_f32())
    }

    /// Forget accumulated time (e.g. after loading, so the game doesn't fast-forward).
    pub fn reset(&mut self) {
        self.started = false;
        self.acc = Duration::ZERO;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(v: u64) -> Duration {
        Duration::from_millis(v)
    }

    #[test]
    fn first_frame_simulates_once() {
        let mut c = FixedStep::new(30, 4);
        assert_eq!(c.advance(ms(500)).0, 1);
    }

    #[test]
    fn steps_at_30hz_regardless_of_display_rate() {
        for fps in [30u64, 60, 144, 240] {
            let mut c = FixedStep::new(30, 4);
            c.advance(Duration::ZERO);
            let frame = Duration::from_secs(1) / fps as u32;
            let steps: u32 = (0..fps).map(|_| c.advance(frame).0).sum();
            assert!((29..=31).contains(&steps), "{fps} fps: {steps} steps in one second");
        }
    }

    #[test]
    fn alpha_rises_between_steps() {
        let mut c = FixedStep::new(30, 4);
        c.advance(Duration::ZERO);
        let frame = Duration::from_secs(1) / 144;
        let (s1, a1) = c.advance(frame);
        let (s2, a2) = c.advance(frame);
        assert_eq!((s1, s2), (0, 0));
        assert!(a1 > 0.0 && a2 > a1 && a2 < 1.0, "{a1} {a2}");
    }

    #[test]
    fn long_stall_is_capped() {
        let mut c = FixedStep::new(30, 4);
        c.advance(Duration::ZERO);
        let (steps, alpha) = c.advance(Duration::from_secs(10));
        assert_eq!(steps, 4);
        assert!((0.0..1.0).contains(&alpha));
        // And it doesn't keep catching up afterwards.
        assert!(c.advance(ms(1)).0 <= 1);
    }

    #[test]
    fn reset_restarts_with_one_step() {
        let mut c = FixedStep::new(30, 4);
        c.advance(Duration::ZERO);
        c.advance(ms(20));
        c.reset();
        assert_eq!(c.advance(ms(1000)), (1, 0.0));
    }
}
