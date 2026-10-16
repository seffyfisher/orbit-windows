//! Physical desktop coordinates. Negative origins are valid on secondary displays.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl Rect {
    pub fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
    pub fn right(self) -> i32 {
        self.x + self.width
    }
    pub fn bottom(self) -> i32 {
        self.y + self.height
    }
    pub fn inset(self, amount: i32) -> Self {
        let n = amount
            .max(0)
            .min((self.width.min(self.height) - 1).max(0) / 2);
        Self::new(
            self.x + n,
            self.y + n,
            self.width - n * 2,
            self.height - n * 2,
        )
    }
    pub fn contains(self, x: i32, y: i32) -> bool {
        x >= self.x && y >= self.y && x < self.right() && y < self.bottom()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layout {
    Maximize,
    TopRight,
    Right,
    BottomRight,
    Center,
    BottomLeft,
    Left,
    TopLeft,
}

impl Layout {
    pub const ALL: [Self; 8] = [
        Self::Maximize,
        Self::TopRight,
        Self::Right,
        Self::BottomRight,
        Self::Center,
        Self::BottomLeft,
        Self::Left,
        Self::TopLeft,
    ];

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|v| *v == self).unwrap()
    }
    pub fn label(self, variant: i32) -> &'static str {
        match self {
            Self::Maximize => match variant.rem_euclid(3) {
                1 => "Top half",
                2 => "Bottom half",
                _ => "Maximize",
            },
            Self::Center => "Center",
            Self::Left => match variant.rem_euclid(3) {
                1 => "Left two thirds",
                2 => "Left third",
                _ => "Left half",
            },
            Self::Right => match variant.rem_euclid(3) {
                1 => "Right two thirds",
                2 => "Right third",
                _ => "Right half",
            },
            Self::TopLeft => "Top left",
            Self::TopRight => "Top right",
            Self::BottomLeft => "Bottom left",
            Self::BottomRight => "Bottom right",
        }
    }

    /// Layout edges use integer fractions, so odd-width monitors have no lost column.
    pub fn frame(self, work: Rect, original: Rect, gap: i32, variant: i32) -> Rect {
        let area = work.inset(gap);
        let split_x = area.x + area.width / 2;
        let split_y = area.y + area.height / 2;
        let half_gap = gap / 2;
        match self {
            Self::Maximize => match variant.rem_euclid(3) {
                1 => Rect::new(
                    area.x,
                    area.y,
                    area.width,
                    (area.height / 2 - half_gap).max(1),
                ),
                2 => Rect::new(
                    area.x,
                    split_y + half_gap,
                    area.width,
                    (area.bottom() - split_y - half_gap).max(1),
                ),
                _ => work,
            },
            Self::Center => {
                let w = original.width.min(area.width).max(1);
                let h = original.height.min(area.height).max(1);
                Rect::new(
                    area.x + (area.width - w) / 2,
                    area.y + (area.height - h) / 2,
                    w,
                    h,
                )
            }
            Self::Left | Self::Right => {
                let divisor = if variant.rem_euclid(3) == 0 { 2 } else { 3 };
                let parts = if variant.rem_euclid(3) == 1 { 2 } else { 1 };
                let width = (if self == Self::Left {
                    area.width * parts / divisor
                } else {
                    area.width - area.width * (divisor - parts) / divisor
                } - half_gap)
                    .max(1);
                Rect::new(
                    if self == Self::Left {
                        area.x
                    } else {
                        area.right() - width
                    },
                    area.y,
                    width,
                    area.height,
                )
            }
            _ => {
                let left = matches!(self, Self::TopLeft | Self::BottomLeft);
                let top = matches!(self, Self::TopLeft | Self::TopRight);
                let x = if left { area.x } else { split_x + half_gap };
                let y = if top { area.y } else { split_y + half_gap };
                let right = if left {
                    split_x - half_gap
                } else {
                    area.right()
                };
                let bottom = if top {
                    split_y - half_gap
                } else {
                    area.bottom()
                };
                Rect::new(x, y, (right - x).max(1), (bottom - y).max(1))
            }
        }
    }
}

/// Eight compass sectors with a dead zone and angular hysteresis to prevent flicker.
pub fn select(dx: f64, dy: f64, dead_zone: f64, previous: Option<Layout>) -> Option<Layout> {
    if dx.hypot(dy) < dead_zone {
        return None;
    }
    let angle = (dx.atan2(-dy)).rem_euclid(std::f64::consts::TAU);
    let sector = std::f64::consts::FRAC_PI_4;
    if let Some(old) = previous {
        let delta = (angle - old.index() as f64 * sector + std::f64::consts::PI)
            .rem_euclid(std::f64::consts::TAU)
            - std::f64::consts::PI;
        if delta.abs() < sector / 2.0 + 0.055 {
            return previous;
        }
    }
    Some(Layout::ALL[((angle + sector / 2.0) / sector) as usize % 8])
}

/// Move a restored window between differently sized/scaled monitor work areas.
pub fn transfer(frame: Rect, from: Rect, to: Rect) -> Rect {
    if from.width <= 0 || from.height <= 0 {
        return to;
    }
    let sx = to.width as f64 / from.width as f64;
    let sy = to.height as f64 / from.height as f64;
    let w = ((frame.width as f64 * sx).round() as i32).clamp(1, to.width.max(1));
    let h = ((frame.height as f64 * sy).round() as i32).clamp(1, to.height.max(1));
    let x = to.x + ((frame.x - from.x) as f64 * sx).round() as i32;
    let y = to.y + ((frame.y - from.y) as f64 * sy).round() as i32;
    Rect::new(
        x.clamp(to.x, to.right() - w),
        y.clamp(to.y, to.bottom() - h),
        w,
        h,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn directions_and_cancel() {
        assert_eq!(select(0., 0., 30., None), None);
        for (dx, dy, expected) in [
            (0., -100., Layout::Maximize),
            (100., -100., Layout::TopRight),
            (100., 0., Layout::Right),
            (100., 100., Layout::BottomRight),
            (0., 100., Layout::Center),
            (-100., 100., Layout::BottomLeft),
            (-100., 0., Layout::Left),
            (-100., -100., Layout::TopLeft),
        ] {
            assert_eq!(select(dx, dy, 30., None), Some(expected));
        }
    }
    #[test]
    fn hysteresis_keeps_selection_near_boundary() {
        let a = std::f64::consts::PI / 8. + 0.02;
        assert_eq!(
            select(a.sin() * 100., -a.cos() * 100., 30., Some(Layout::Maximize)),
            Some(Layout::Maximize)
        );
        assert_eq!(
            select(a.sin() * 100., -a.cos() * 100., 30., None),
            Some(Layout::TopRight)
        );
    }
    #[test]
    fn odd_negative_monitor_tiles_cover_work_area() {
        let work = Rect::new(-1919, -200, 1919, 1041);
        let original = Rect::new(0, 0, 800, 600);
        let tl = Layout::TopLeft.frame(work, original, 0, 0);
        let br = Layout::BottomRight.frame(work, original, 0, 0);
        assert_eq!(tl.right(), br.x);
        assert_eq!(tl.bottom(), br.y);
        assert_eq!(br.right(), work.right());
        assert_eq!(br.bottom(), work.bottom());
        let left = Layout::Left.frame(work, original, 0, 0);
        let right = Layout::Right.frame(work, original, 0, 0);
        assert_eq!(left.right(), right.x);
        assert_eq!(left.width + right.width, work.width);
    }
    #[test]
    fn layouts_stay_inside_work_area_with_gaps() {
        for work in [
            Rect::new(-2560, 0, 2560, 1400),
            Rect::new(0, -1080, 1080, 1920),
            Rect::new(0, 40, 1537, 824),
        ] {
            for gap in [0, 8, 24] {
                for layout in Layout::ALL {
                    for variant in -3..4 {
                        let r = layout.frame(work, Rect::new(10, 10, 1800, 1000), gap, variant);
                        assert!(
                            r.x >= work.x
                                && r.y >= work.y
                                && r.right() <= work.right()
                                && r.bottom() <= work.bottom()
                        );
                        assert!(r.width > 0 && r.height > 0);
                    }
                }
            }
        }
    }
    #[test]
    fn monitor_transfer_respects_taskbar_and_negative_origin() {
        let from = Rect::new(0, 40, 1920, 1040);
        let to = Rect::new(-2560, -200, 2560, 1400);
        assert_eq!(transfer(from, from, to), to);
        let r = transfer(Rect::new(2000, 1200, 800, 600), from, to);
        assert!(r.x >= to.x && r.y >= to.y && r.right() <= to.right() && r.bottom() <= to.bottom());
    }
}
