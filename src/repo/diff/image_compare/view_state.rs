//! How the pictures being compared are looked at, shared by every mode: the zoom and the pan,
//! the mode, and where the swipe and the onion skin are.

use egui::{Pos2, Rect, Vec2, pos2, vec2};

/// The ways two pictures are compared.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    /// Side by side.
    #[default]
    TwoUp,
    /// One over the other, a line to drag between them.
    Swipe,
    /// The new one over the old, as see-through as asked.
    Onion,
    /// Where they differ.
    Difference,
}

impl Mode {
    pub const ALL: [Mode; 4] = [Mode::TwoUp, Mode::Swipe, Mode::Onion, Mode::Difference];

    pub fn label(self) -> &'static str {
        match self {
            Mode::TwoUp => "2-up",
            Mode::Swipe => "Swipe",
            Mode::Onion => "Onion skin",
            Mode::Difference => "Difference",
        }
    }
}

/// What the viewer has set, kept per file while it is open.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewState {
    pub mode: Mode,
    /// Screen points per picture pixel; `None` fits the picture to its area.
    pub zoom: Option<f32>,
    /// The picture point at the centre of the area, as a share of its size (0.5, 0.5 the
    /// middle).
    pub center: Vec2,
    /// Swipe: where the line is, 0 (all new) to 1 (all old).
    pub swipe: f32,
    /// Onion skin: how much the new picture shows, 0 to 1.
    pub onion: f32,
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            mode: Mode::TwoUp,
            zoom: None,
            center: vec2(0.5, 0.5),
            swipe: 0.5,
            onion: 0.5,
        }
    }
}

pub const MIN_ZOOM: f32 = 0.02;
pub const MAX_ZOOM: f32 = 64.0;

/// The scale that fits `size` into `area` (never above 1: small pictures stay pixel for pixel).
pub fn fit_scale(size: Vec2, area: Vec2) -> f32 {
    if size.x <= 0.0 || size.y <= 0.0 {
        return 1.0;
    }
    (area.x / size.x).min(area.y / size.y).min(1.0)
}

impl ViewState {
    /// The scale the picture of `size` is shown at in `area`.
    pub fn scale(&self, size: Vec2, area: Vec2) -> f32 {
        self.zoom.unwrap_or_else(|| fit_scale(size, area))
    }

    /// Where the picture of `size` is drawn in `area`.
    pub fn placed(&self, size: Vec2, area: Rect) -> Rect {
        let scale = self.scale(size, area.size());
        let shown = size * scale;
        let center = if self.zoom.is_none() {
            vec2(0.5, 0.5)
        } else {
            self.center
        };
        let min = area.center() - vec2(center.x * shown.x, center.y * shown.y);
        Rect::from_min_size(min, shown)
    }

    /// Zooms by `factor` keeping the picture point under `pointer` where it is.
    pub fn zoom_at(&mut self, factor: f32, pointer: Pos2, size: Vec2, area: Rect) {
        let before = self.placed(size, area);
        let scale = self.scale(size, area.size());
        let new = (scale * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        // The point under the pointer, as a share of the picture.
        let at = vec2(
            (pointer.x - before.min.x) / before.width().max(1.0),
            (pointer.y - before.min.y) / before.height().max(1.0),
        );
        let shown = size * new;
        // That point stays under the pointer: centre = at − (pointer − area centre) / shown.
        let off = pointer - area.center();
        self.center = vec2(
            at.x - off.x / shown.x.max(1.0),
            at.y - off.y / shown.y.max(1.0),
        );
        self.zoom = Some(new);
    }

    /// Moves the picture by `delta` screen points.
    pub fn pan(&mut self, delta: Vec2, size: Vec2, area: Rect) {
        if self.zoom.is_none() {
            self.zoom = Some(self.scale(size, area.size()));
            self.center = vec2(0.5, 0.5);
        }
        let shown = size * self.scale(size, area.size());
        self.center -= vec2(delta.x / shown.x.max(1.0), delta.y / shown.y.max(1.0));
        self.center = self.center.clamp(vec2(-0.5, -0.5), vec2(1.5, 1.5));
    }

    /// The zoom as people read it, in screen pixels per picture pixel (`ppp`: pixels per
    /// point): "Fit", "100%", "250%".
    pub fn zoom_label(&self, ppp: f32) -> String {
        match self.zoom {
            None => "Fit".to_owned(),
            Some(z) => format!("{:.0}%", z * ppp * 100.0),
        }
    }
}

/// The part of the picture's texture (0 to 1) left of the screen x `split` in `placed`.
pub fn left_of(placed: Rect, split: f32) -> (Rect, Rect) {
    let t = ((split - placed.left()) / placed.width().max(1.0)).clamp(0.0, 1.0);
    let screen = Rect::from_min_max(
        placed.min,
        pos2(placed.left() + placed.width() * t, placed.bottom()),
    );
    let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(t, 1.0));
    (screen, uv)
}

#[cfg(test)]
mod tests {
    use super::*;

    const AREA: Rect = Rect::from_min_max(pos2(0.0, 0.0), pos2(400.0, 300.0));

    #[test]
    fn fitting_never_enlarges() {
        assert_eq!(fit_scale(vec2(800.0, 300.0), vec2(400.0, 300.0)), 0.5);
        assert_eq!(fit_scale(vec2(10.0, 10.0), vec2(400.0, 300.0)), 1.0);
        assert_eq!(fit_scale(vec2(0.0, 10.0), vec2(400.0, 300.0)), 1.0);
        let v = ViewState::default();
        assert_eq!(
            v.placed(vec2(800.0, 300.0), AREA),
            Rect::from_min_size(pos2(0.0, 75.0), vec2(400.0, 150.0))
        );
        assert_eq!(v.zoom_label(2.0), "Fit");
    }

    #[test]
    fn zooming_keeps_the_point_under_the_pointer() {
        let size = vec2(200.0, 100.0);
        let mut v = ViewState::default();
        let pointer = pos2(250.0, 160.0);
        let before = v.placed(size, AREA);
        let share = (pointer - before.min) / before.size();
        v.zoom_at(3.0, pointer, size, AREA);
        let after = v.placed(size, AREA);
        let under = after.min + share * after.size();
        assert!((under - pointer).length() < 0.01, "{under:?}");
        assert_eq!(v.zoom_label(1.0), "300%");
        assert_eq!(v.zoom_label(2.0), "600%");
        v.zoom_at(1e9, pointer, size, AREA);
        assert_eq!(v.zoom, Some(MAX_ZOOM));
    }

    #[test]
    fn panning_moves_by_screen_points() {
        let size = vec2(200.0, 100.0);
        let mut v = ViewState::default();
        let before = v.placed(size, AREA);
        v.pan(vec2(30.0, -10.0), size, AREA);
        let after = v.placed(size, AREA);
        assert!((after.min - before.min - vec2(30.0, -10.0)).length() < 0.01);
    }

    #[test]
    fn the_swipe_splits_the_picture() {
        let placed = Rect::from_min_size(pos2(100.0, 0.0), vec2(200.0, 50.0));
        let (screen, uv) = left_of(placed, 150.0);
        assert_eq!(screen.width(), 50.0);
        assert_eq!(uv.max.x, 0.25);
        assert_eq!(left_of(placed, 0.0).1.max.x, 0.0);
        assert_eq!(left_of(placed, 900.0).1.max.x, 1.0);
    }
}
