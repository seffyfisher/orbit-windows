//! Gesture state is independent of Win32. Unwrapping only reveals the choices.
//! Installation requires a separate explicit button. No idle animation timer.
#[derive(Debug, Default)]
pub struct Tear {
    pub progress: f32,
    drag: Option<(f32, f32, f32)>,
}

impl Tear {
    pub fn begin(&mut self, x: f32, y: f32) {
        self.drag = Some((x, y, self.progress));
    }
    pub fn update(&mut self, x: f32, y: f32, width: f32) {
        if let Some((start_x, start_y, start_progress)) = self.drag {
            let travel = (start_x - x) * 0.85 + (y - start_y) * 0.5;
            self.progress = self
                .progress
                .max((start_progress + travel / (width * 0.7)).clamp(0., 1.));
        }
    }
    pub fn release(&mut self) -> bool {
        let was_dragging = self.drag.take().is_some();
        was_dragging && self.progress >= 0.94
    }
    pub fn cancel(&mut self) {
        self.drag = None;
    }
    pub fn dragging(&self) -> bool {
        self.drag.is_some()
    }
}

/// Jagged diagonal cut through the actual paper texture, normalized coordinates.
/// The cut opens from the upper-right corner towards the lower-left corner.
pub fn tear_polygon(progress: f32) -> Vec<(f32, f32)> {
    if progress <= 0. {
        return Vec::new();
    }
    let left = (1. - progress * 1.14).max(-0.05);
    let radius = 0.02 + 0.34 * progress + (progress - 0.72).max(0.) * 2.7;
    let edge = |i: usize, lower: bool| {
        let x = left + (1.06 - left) * i as f32 / 90.;
        let jag = (((i * 17 + 3) % 13) as f32 / 13. - 0.5) * 0.008;
        let spread = radius * ((x - left) / (1.06 - left)).sqrt();
        (
            x,
            0.82 - x * 0.62 + jag + if lower { spread } else { -spread },
        )
    };
    (0..=90)
        .map(|i| edge(i, false))
        .chain((0..=90).rev().map(|i| edge(i, true)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn partial_release_resume_and_focus_loss_never_install() {
        let mut tear = Tear::default();
        tear.begin(700., 100.);
        assert!(tear.dragging());
        tear.update(400., 140., 840.);
        assert!(tear.progress > 0.4 && tear.progress < 0.5);
        assert!(!tear.release());
        let partial = tear.progress;
        tear.begin(400., 140.);
        tear.update(450., 100., 840.);
        assert_eq!(tear.progress, partial); // paper cannot become intact again
        tear.cancel();
        assert!(!tear.release());
        tear.begin(400., 140.);
        tear.update(-40., 200., 840.);
        assert!(tear.release());
        assert!(!tear.release()); // duplicate mouse-up cannot install twice
    }
    #[test]
    fn a_click_or_wrong_direction_does_not_install() {
        let mut tear = Tear::default();
        tear.begin(100., 100.);
        assert!(!tear.release());
        tear.begin(100., 100.);
        tear.update(900., 0., 840.);
        assert_eq!(tear.progress, 0.);
        assert!(!tear.release());
        assert!(tear_polygon(0.).is_empty());
        assert!(
            tear_polygon(0.5)
                .iter()
                .all(|(x, y)| x.is_finite() && y.is_finite())
        );
    }
}
