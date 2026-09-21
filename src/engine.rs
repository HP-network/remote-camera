use std::time::{Duration, Instant};

use crate::config::Config;
use crate::filter::{Motion, MotionFilter};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineState {
    Disabled,
    WaitingForTarget,
    Tracking,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TargetWindow {
    pub id: usize,
    pub center_x: i32,
    pub center_y: i32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OutputMotion {
    pub motion: Motion,
    pub recenter: bool,
    pub swallow: bool,
}

pub struct CameraEngine {
    config: Config,
    filter: MotionFilter,
    state: EngineState,
    target: Option<TargetWindow>,
    last_cursor: Option<(i32, i32)>,
    last_recenter: Option<Instant>,
    swallow_until: Option<Instant>,
}

impl CameraEngine {
    pub fn new(config: Config) -> Self {
        Self {
            filter: MotionFilter::new(config.filter_config()),
            config,
            state: EngineState::WaitingForTarget,
            target: None,
            last_cursor: None,
            last_recenter: None,
            swallow_until: None,
        }
    }

    pub fn state(&self) -> EngineState {
        self.state
    }

    pub fn target_changed(&mut self, target: Option<TargetWindow>) -> bool {
        if self.target == target {
            return false;
        }
        self.target = target;
        self.filter.reset();
        self.swallow_until = None;
        self.last_recenter = None;
        self.last_cursor = None;
        self.state = match self.target {
            Some(_) if self.state != EngineState::Disabled => EngineState::Tracking,
            Some(_) => EngineState::Disabled,
            None if self.state != EngineState::Disabled => EngineState::WaitingForTarget,
            None => EngineState::Disabled,
        };
        true
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.state = if !enabled {
            EngineState::Disabled
        } else if self.target.is_some() {
            EngineState::Tracking
        } else {
            EngineState::WaitingForTarget
        };
        self.filter.reset();
        self.swallow_until = None;
        self.last_cursor = None;
    }

    pub fn is_enabled(&self) -> bool {
        self.state != EngineState::Disabled
    }

    pub fn update_config(&mut self, config: Config) {
        self.filter = MotionFilter::new(config.filter_config());
        self.config = config;
        self.swallow_until = None;
    }

    pub fn on_cursor_position(&mut self, x: i32, y: i32, now: Instant) -> Option<OutputMotion> {
        if self.state != EngineState::Tracking {
            return None;
        }
        let target = self.target?;
        if self.config.recenter_cursor
            && self.swallow_until.is_some_and(|deadline| {
                now <= deadline && near(x, y, target.center_x, target.center_y, 2)
            })
        {
            self.swallow_until = None;
            return Some(OutputMotion {
                motion: Motion::default(),
                recenter: false,
                swallow: true,
            });
        }

        let (dx, dy) = if self.config.recenter_cursor {
            (
                x.saturating_sub(target.center_x),
                y.saturating_sub(target.center_y),
            )
        } else {
            let previous = self.last_cursor.replace((x, y));
            let Some((previous_x, previous_y)) = previous else {
                return Some(OutputMotion {
                    motion: Motion::default(),
                    recenter: false,
                    swallow: true,
                });
            };
            (x.saturating_sub(previous_x), y.saturating_sub(previous_y))
        };
        if dx.unsigned_abs() > self.config.max_delta as u32 * 4
            || dy.unsigned_abs() > self.config.max_delta as u32 * 4
        {
            self.filter.reset();
            return Some(self.recenter_output(now));
        }

        let motion = self.filter.push(Motion {
            dx: f64::from(dx),
            dy: f64::from(dy),
        });
        Some(OutputMotion {
            motion,
            recenter: self.config.recenter_cursor,
            swallow: true,
        })
    }

    pub fn on_recenter_complete(&mut self, now: Instant) {
        self.last_recenter = Some(now);
        self.swallow_until = Some(now + Duration::from_millis(self.config.recenter_settle_ms));
    }

    fn recenter_output(&mut self, now: Instant) -> OutputMotion {
        self.on_recenter_complete(now);
        OutputMotion {
            motion: Motion::default(),
            recenter: self.config.recenter_cursor,
            swallow: true,
        }
    }
}

fn near(x: i32, y: i32, target_x: i32, target_y: i32, tolerance: i32) -> bool {
    (x - target_x).abs() <= tolerance && (y - target_y).abs() <= tolerance
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine() -> CameraEngine {
        let config = Config {
            require_rdp: false,
            recenter_settle_ms: 10,
            ..Config::default()
        };
        CameraEngine::new(config)
    }

    #[test]
    fn waits_until_a_window_is_available() {
        let mut engine = engine();
        assert_eq!(engine.state(), EngineState::WaitingForTarget);
        assert!(engine.on_cursor_position(10, 10, Instant::now()).is_none());
    }

    #[test]
    fn target_change_resets_filter_and_tracks() {
        let mut engine = engine();
        assert!(engine.target_changed(Some(TargetWindow {
            id: 4,
            center_x: 100,
            center_y: 100
        })));
        assert_eq!(engine.state(), EngineState::Tracking);
        let output = engine.on_cursor_position(110, 96, Instant::now()).unwrap();
        assert!(output.swallow);
        assert!(output.motion.dx > 0.0);
    }

    #[test]
    fn injected_recenter_event_is_swallowed_once() {
        let mut engine = engine();
        engine.target_changed(Some(TargetWindow {
            id: 4,
            center_x: 100,
            center_y: 100,
        }));
        let now = Instant::now();
        engine.on_recenter_complete(now);
        let output = engine
            .on_cursor_position(100, 100, now + Duration::from_millis(1))
            .unwrap();
        assert!(output.swallow);
        assert_eq!(output.motion, Motion::default());
    }

    #[test]
    fn relative_mode_uses_mouse_delta_without_recenter() {
        let config = Config {
            require_rdp: false,
            recenter_cursor: false,
            ..Config::default()
        };
        let mut engine = CameraEngine::new(config);
        engine.target_changed(Some(TargetWindow {
            id: 8,
            center_x: 100,
            center_y: 100,
        }));
        let now = Instant::now();
        let first = engine.on_cursor_position(500, 400, now).unwrap();
        assert_eq!(first.motion, Motion::default());
        let second = engine
            .on_cursor_position(510, 396, now + Duration::from_millis(8))
            .unwrap();
        assert!(second.motion.dx > 0.0);
        assert!(second.motion.dy < 0.0);
        assert!(!second.recenter);
    }
}
