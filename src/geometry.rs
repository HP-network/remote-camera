#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bounds {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Bounds {
    pub fn center(self) -> Option<(i32, i32)> {
        let width = self.right.checked_sub(self.left)?;
        let height = self.bottom.checked_sub(self.top)?;
        if width <= 0 || height <= 0 {
            return None;
        }
        Some((
            self.left.saturating_add(width / 2),
            self.top.saturating_add(height / 2),
        ))
    }
}

pub fn recenter_point(
    clipped: Option<Bounds>,
    window: Option<Bounds>,
    desktop: Bounds,
) -> Option<(i32, i32)> {
    clipped
        .filter(|bounds| *bounds != desktop)
        .and_then(Bounds::center)
        .or_else(|| window.and_then(Bounds::center))
        .or_else(|| desktop.center())
}

#[cfg(test)]
mod tests {
    use super::*;

    const DESKTOP: Bounds = Bounds {
        left: 0,
        top: 0,
        right: 1920,
        bottom: 1080,
    };

    #[test]
    fn centers_inside_active_cursor_clip() {
        let clipped = Bounds {
            left: 200,
            top: 100,
            right: 1200,
            bottom: 900,
        };
        assert_eq!(
            recenter_point(Some(clipped), None, DESKTOP),
            Some((700, 500))
        );
    }

    #[test]
    fn uses_game_client_when_cursor_is_not_clipped() {
        let game = Bounds {
            left: 300,
            top: 200,
            right: 1500,
            bottom: 900,
        };
        assert_eq!(
            recenter_point(Some(DESKTOP), Some(game), DESKTOP),
            Some((900, 550))
        );
    }

    #[test]
    fn falls_back_to_desktop_center() {
        assert_eq!(recenter_point(None, None, DESKTOP), Some((960, 540)));
    }

    #[test]
    fn ignores_invalid_clip_and_window_bounds() {
        let invalid = Bounds {
            left: 10,
            top: 10,
            right: 10,
            bottom: 20,
        };
        assert_eq!(
            recenter_point(Some(invalid), Some(invalid), DESKTOP),
            Some((960, 540))
        );
    }
}
