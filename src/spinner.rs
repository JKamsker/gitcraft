//! egui's spinner, drawn the same, but at a frame rate a spinner needs.
//!
//! `egui::Spinner` asks for the next frame straight away, so while one is in sight the whole
//! window is drawn again at the display's refresh rate: on integrated graphics that keeps the
//! GPU busy for as long as anything loads, fetches or a CI check runs. This one asks for its
//! next frame after [`FRAME`], or after [`FRAME_UNFOCUSED`] while the window isn't focused.

use std::time::Duration;

use egui::{Color32, Pos2, Rect, Response, Sense, Shape, Stroke, Ui, Widget, WidgetInfo};
use egui::{WidgetType, emath::lerp, vec2};

/// 20 frames a second: the arc turns once a second, so that is a step of 18°.
const FRAME: Duration = Duration::from_millis(50);
const FRAME_UNFOCUSED: Duration = Duration::from_millis(250);

#[must_use = "put it in a ui with `ui.add(spinner)`"]
#[derive(Default)]
pub struct Spinner {
    size: Option<f32>,
    color: Option<Color32>,
}

impl Spinner {
    pub fn new() -> Self {
        Self::default()
    }

    /// Width and height; the style's `interact_size` when not given.
    #[inline]
    pub fn size(mut self, size: f32) -> Self {
        self.size = Some(size);
        self
    }

    #[inline]
    pub fn color(mut self, color: impl Into<Color32>) -> Self {
        self.color = Some(color.into());
        self
    }

    pub fn paint_at(&self, ui: &Ui, rect: Rect) {
        if !ui.is_rect_visible(rect) || waiting_out_grace(ui, rect) {
            return;
        }
        let focused = ui.input(|i| i.viewport().focused) != Some(false);
        ui.ctx()
            .request_repaint_after(if focused { FRAME } else { FRAME_UNFOCUSED });

        let color = self
            .color
            .unwrap_or_else(|| ui.visuals().strong_text_color());
        let radius = (rect.height().min(rect.width()) / 2.0) - 2.0;
        let n_points = (radius.round() as u32).clamp(8, 128);
        let time = ui.input(|i| i.time);
        let start_angle = time * std::f64::consts::TAU;
        let end_angle = start_angle + 240f64.to_radians() * time.sin();
        let points: Vec<Pos2> = (0..n_points)
            .map(|i| {
                let angle = lerp(start_angle..=end_angle, f64::from(i) / f64::from(n_points));
                let (sin, cos) = angle.sin_cos();
                rect.center() + radius * vec2(cos as f32, sin as f32)
            })
            .collect();
        ui.painter()
            .add(Shape::line(points, Stroke::new(3.0, color)));
    }
}

/// Whether a spinner at `rect` has been wanted for less than [`GRACE`]: most reads end sooner,
/// and a spinner that flashes up and goes reads as slowness rather than speed. One that went
/// out of sight (or moved) for longer than [`GAP`] starts its grace again.
fn waiting_out_grace(ui: &Ui, rect: Rect) -> bool {
    let now = ui.input(|i| i.time);
    let id = ui.id().with((
        "spinner-grace",
        (rect.center().x / 8.0).round() as i32,
        (rect.center().y / 8.0).round() as i32,
    ));
    let first = ui.data_mut(|d| {
        let (first, last) = d.get_temp::<(f64, f64)>(id).unwrap_or((now, now));
        let first = if now - last > GAP { now } else { first };
        d.insert_temp(id, (first, now));
        first
    });
    let waited = now - first;
    if waited >= GRACE {
        return false;
    }
    // Drawn again soon enough to stay "in sight" and to show once the grace is over.
    ui.ctx()
        .request_repaint_after(Duration::from_secs_f64((GRACE - waited).min(GAP / 2.0)));
    true
}

/// How long something loads before a spinner says so.
const GRACE: f64 = 1.0;
/// How long a spinner may be out of sight and still count as the same wait.
const GAP: f64 = 0.4;

impl Widget for Spinner {
    fn ui(self, ui: &mut Ui) -> Response {
        let size = self
            .size
            .unwrap_or_else(|| ui.style().spacing.interact_size.y);
        let (rect, response) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
        response.widget_info(|| WidgetInfo::new(WidgetType::ProgressIndicator));
        self.paint_at(ui, rect);
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{RawInput, ViewportId};

    /// How long the frame with a spinner asks to wait for the next one.
    fn next_frame_after(focused: Option<bool>) -> Duration {
        let ctx = egui::Context::default();
        let frame = |ctx: &egui::Context, time: f64| {
            let mut input = RawInput {
                time: Some(time),
                ..RawInput::default()
            };
            if let Some(viewport) = input.viewports.get_mut(&ViewportId::ROOT) {
                viewport.focused = focused;
            }
            let mut output = ctx.run_ui(input, |ui| {
                ui.add(Spinner::new());
            });
            // Nothing renders here: the font texture is left unapplied on purpose.
            output.textures_delta.clear();
            output
        };
        // The first frames of a context ask for more of their own accord; and the spinner shows
        // only once its grace is over, in frames close enough together to be one wait.
        let mut delay = Duration::ZERO;
        let frames = (GRACE / (GAP / 2.0)).ceil() as usize + 2;
        for i in 0..frames {
            delay = frame(&ctx, i as f64 * GAP / 2.0)
                .viewport_output
                .get(&ViewportId::ROOT)
                .map(|viewport| viewport.repaint_delay)
                .unwrap_or_default();
        }
        delay
    }

    /// egui starts the next frame early by the time it expects one to take.
    fn assert_waits(delay: Duration, wanted: Duration) {
        let frame_time = Duration::from_secs_f32(RawInput::default().predicted_dt);
        let early = wanted.saturating_sub(delay);
        assert!(
            delay > Duration::ZERO && early <= frame_time + Duration::from_millis(1),
            "waits {delay:?}, wanted {wanted:?}"
        );
    }

    #[test]
    fn a_spinner_shows_only_after_its_grace() {
        let ctx = egui::Context::default();
        let waiting = |time: f64| {
            let mut waiting = false;
            let output = ctx.run_ui(
                RawInput {
                    time: Some(time),
                    ..RawInput::default()
                },
                |ui| {
                    let rect = egui::Rect::from_min_size(egui::pos2(10.0, 10.0), vec2(20.0, 20.0));
                    waiting = waiting_out_grace(ui, rect);
                },
            );
            output.drop_without_applying_deltas();
            waiting
        };
        // Frames close enough together to be one wait.
        let step = GAP / 2.0;
        let mut time = 0.0;
        while time < GRACE {
            assert!(waiting(time));
            time += step;
        }
        assert!(!waiting(time));
        // Out of sight for a while: a new wait, with its grace again.
        assert!(waiting(time + GAP * 2.0));
    }

    #[test]
    fn spins_at_its_own_frame_rate() {
        assert_waits(next_frame_after(Some(true)), FRAME);
        assert_waits(next_frame_after(None), FRAME);
        assert_waits(next_frame_after(Some(false)), FRAME_UNFOCUSED);
    }
}
