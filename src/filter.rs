use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FilterConfig {
    pub min_cutoff: f64,
    pub beta: f64,
    pub derivative_cutoff: f64,
    pub deadzone: f64,
    pub max_delta: f64,
    pub max_output: f64,
    pub sensitivity: f64,
}

impl Default for FilterConfig {
    fn default() -> Self {
        Self {
            min_cutoff: 1.2,
            beta: 0.015,
            derivative_cutoff: 1.0,
            deadzone: 0.15,
            max_delta: 300.0,
            max_output: 40.0,
            sensitivity: 1.0,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Motion {
    pub dx: f64,
    pub dy: f64,
}

#[derive(Debug, Clone, Copy)]
struct OneEuro {
    min_cutoff: f64,
    beta: f64,
    derivative_cutoff: f64,
    previous_raw: Option<f64>,
    previous_filtered: Option<f64>,
    previous_derivative: f64,
}

impl OneEuro {
    fn new(config: FilterConfig) -> Self {
        Self {
            min_cutoff: config.min_cutoff,
            beta: config.beta,
            derivative_cutoff: config.derivative_cutoff,
            previous_raw: None,
            previous_filtered: None,
            previous_derivative: 0.0,
        }
    }

    fn reset(&mut self) {
        self.previous_raw = None;
        self.previous_filtered = None;
        self.previous_derivative = 0.0;
    }

    fn filter(&mut self, value: f64, dt: Duration) -> f64 {
        let seconds = dt.as_secs_f64().clamp(0.000_5, 0.25);
        let derivative = self
            .previous_raw
            .map(|previous| (value - previous) / seconds)
            .unwrap_or(0.0);
        let derivative_alpha = smoothing_factor(seconds, self.derivative_cutoff);
        self.previous_derivative += (derivative - self.previous_derivative) * derivative_alpha;
        let cutoff = self.min_cutoff + self.beta * self.previous_derivative.abs();
        let alpha = smoothing_factor(seconds, cutoff);
        let filtered = self
            .previous_filtered
            .map(|previous| previous + alpha * (value - previous))
            .unwrap_or(value);
        self.previous_raw = Some(value);
        self.previous_filtered = Some(filtered);
        filtered
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MotionFilter {
    config: FilterConfig,
    x: OneEuro,
    y: OneEuro,
    last_sample: Option<Instant>,
}

impl MotionFilter {
    pub fn new(config: FilterConfig) -> Self {
        Self {
            x: OneEuro::new(config),
            y: OneEuro::new(config),
            config,
            last_sample: None,
        }
    }

    pub fn reset(&mut self) {
        self.x.reset();
        self.y.reset();
        self.last_sample = None;
    }

    pub fn push(&mut self, motion: Motion) -> Motion {
        self.push_at(motion, Instant::now())
    }

    pub fn push_at(&mut self, mut motion: Motion, now: Instant) -> Motion {
        let dt = self
            .last_sample
            .map(|last| now.saturating_duration_since(last))
            .unwrap_or(Duration::from_millis(8));
        self.last_sample = Some(now);

        motion.dx = clamp(motion.dx, -self.config.max_delta, self.config.max_delta);
        motion.dy = clamp(motion.dy, -self.config.max_delta, self.config.max_delta);
        motion.dx = apply_deadzone(motion.dx, self.config.deadzone);
        motion.dy = apply_deadzone(motion.dy, self.config.deadzone);

        let dx = self.x.filter(motion.dx * self.config.sensitivity, dt);
        let dy = self.y.filter(motion.dy * self.config.sensitivity, dt);
        Motion {
            dx: clamp(dx, -self.config.max_output, self.config.max_output),
            dy: clamp(dy, -self.config.max_output, self.config.max_output),
        }
    }
}

fn smoothing_factor(seconds: f64, cutoff: f64) -> f64 {
    let tau = 1.0 / (2.0 * std::f64::consts::PI * cutoff.max(0.000_1));
    1.0 / (1.0 + tau / seconds)
}

fn apply_deadzone(value: f64, deadzone: f64) -> f64 {
    if value.abs() < deadzone {
        0.0
    } else {
        value
    }
}

fn clamp(value: f64, min: f64, max: f64) -> f64 {
    value.clamp(min, max)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_small_rdp_jitter() {
        let mut filter = MotionFilter::new(FilterConfig {
            deadzone: 1.0,
            ..Default::default()
        });
        let now = Instant::now();
        let output = filter.push_at(Motion { dx: 0.8, dy: -0.9 }, now);
        assert_eq!(output, Motion::default());
    }

    #[test]
    fn limits_a_remote_desktop_jump() {
        let mut filter = MotionFilter::new(FilterConfig {
            min_cutoff: 30.0,
            max_delta: 12.0,
            max_output: 8.0,
            ..Default::default()
        });
        let output = filter.push_at(
            Motion {
                dx: 1000.0,
                dy: -1000.0,
            },
            Instant::now(),
        );
        assert_eq!(output, Motion { dx: 8.0, dy: -8.0 });
    }

    #[test]
    fn one_euro_damps_direction_changes() {
        let mut filter = MotionFilter::new(FilterConfig {
            min_cutoff: 1.0,
            beta: 0.0,
            deadzone: 0.0,
            ..Default::default()
        });
        let now = Instant::now();
        let first = filter.push_at(Motion { dx: 10.0, dy: 0.0 }, now);
        let second = filter.push_at(
            Motion { dx: -10.0, dy: 0.0 },
            now + Duration::from_millis(8),
        );
        assert!(first.dx > 0.0);
        assert!(second.dx < first.dx);
        assert!(second.dx > -10.0);
    }

    #[test]
    fn reset_forgets_previous_velocity() {
        let mut filter = MotionFilter::new(FilterConfig {
            beta: 0.1,
            ..Default::default()
        });
        let now = Instant::now();
        let _ = filter.push_at(Motion { dx: 20.0, dy: 0.0 }, now);
        filter.reset();
        let output = filter.push_at(Motion { dx: 0.0, dy: 0.0 }, now + Duration::from_secs(1));
        assert_eq!(output, Motion::default());
    }
}
