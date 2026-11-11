//! Cached physical row geometry for soft-wrapped patches. Logical patch indices never change.
use super::{DiffData, Kind, LINE_H, NUM_W};
use egui::{FontId, Ui};
use std::sync::Arc;

#[derive(Clone)]
pub(super) struct Geometry {
    pub offsets: Arc<Vec<f32>>,
    pub text_width: f32,
}
impl Geometry {
    pub fn at(&self, row: usize) -> f32 {
        *self
            .offsets
            .get(row)
            .unwrap_or_else(|| self.offsets.last().unwrap_or(&0.0))
    }
    pub fn row(&self, y: f32) -> usize {
        self.offsets
            .partition_point(|v| *v <= y)
            .saturating_sub(1)
            .min(self.offsets.len().saturating_sub(2))
    }
    pub fn height(&self, row: usize) -> f32 {
        self.at(row + 1) - self.at(row)
    }
    pub fn total(&self) -> f32 {
        self.at(self.offsets.len().saturating_sub(1))
    }
}
/// Converts a physical offset back to the fixed logical row coordinate used by navigation.
pub(in crate::repo) fn logical_offset(offsets: Option<&[f32]>, y: f32) -> f32 {
    offsets.map_or(y, |o| {
        o.partition_point(|v| *v <= y).saturating_sub(1) as f32 * LINE_H
    })
}
pub(super) fn geometry(
    ui: &Ui,
    data: &DiffData,
    split: bool,
    width: f32,
    font: &FontId,
) -> Arc<Geometry> {
    let width = width.max(1.0);
    let key = (
        data.source,
        data.geometry_revision,
        data.lines.len(),
        data.split.len(),
        split,
        width.to_bits(),
        font.size.to_bits(),
        ui.ctx().pixels_per_point().to_bits(),
    );
    let id = ui.id().with("wrapped-patch-geometry");
    if let Some((before, g)) = ui.ctx().data(|d| d.get_temp::<(_, Arc<Geometry>)>(id))
        && key == before
    {
        return g;
    }
    let text_width = if split {
        width / 2.0 - NUM_W - 24.0
    } else {
        width - NUM_W * 2.0 - 24.0
    };
    let height = |i: usize| {
        data.lines.get(i).map_or(LINE_H, |line| {
            if matches!(line.kind, Kind::Hunk | Kind::Meta | Kind::Fold) {
                return LINE_H;
            }
            (galley(ui, &line.text, font, text_width).size().y + 4.0).max(LINE_H)
        })
    };
    let rows = super::presentation::DisplayRows::new(data, split);
    let heights = (0..rows.len()).map(|row| {
        let (a, b) = rows.patch_lines(row).unwrap_or((None, None));
        a.map(height)
            .unwrap_or(LINE_H)
            .max(b.map(height).unwrap_or(LINE_H))
    });
    let mut offsets = Vec::with_capacity(rows.len() + 1);
    offsets.push(0.0);
    for h in heights {
        offsets.push(offsets.last().copied().unwrap_or(0.0) + h);
    }
    let g = Arc::new(Geometry {
        offsets: Arc::new(offsets),
        text_width,
    });
    ui.ctx().data_mut(|d| d.insert_temp(id, (key, g.clone())));
    g
}
pub(super) fn galley(ui: &Ui, text: &str, font: &FontId, width: f32) -> Arc<egui::Galley> {
    let mut job = super::paint::line_job(
        &[(egui::Color32::WHITE, text)],
        None,
        font,
        egui::Color32::TRANSPARENT,
    );
    job.wrap.max_width = width.max(1.0);
    job.wrap.break_anywhere = true;
    ui.fonts_mut(|f| f.layout_job(job))
}
/// One rectangle per visual row, for both selection and search matches.
pub(super) fn rectangles(galley: &egui::Galley, chars: std::ops::Range<usize>) -> Vec<egui::Rect> {
    let mut offset = 0;
    let mut result = Vec::new();
    for row in &galley.rows {
        let n = row.char_count_excluding_newline().0;
        let start = chars.start.max(offset);
        let end = chars.end.min(offset + n);
        if start < end {
            let x1 = row.pos.x + row.x_offset((start - offset).into());
            let x2 = row.pos.x + row.x_offset((end - offset).into());
            result.push(egui::Rect::from_min_max(
                egui::pos2(x1, row.pos.y),
                egui::pos2(x2.max(x1 + 2.0), row.pos.y + row.size.y),
            ));
        }
        offset += n + usize::from(row.ends_with_newline);
    }
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wrapped_geometry_keeps_logical_indices_and_maps_unicode_selection_to_all_rows() {
        let ctx = egui::Context::default();
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            let text = "long αβγ function_name_and_arguments ".repeat(20);
            let galley = galley(ui, &text, &crate::theme::mono(12.0), 130.0);
            assert!(galley.rows.len() > 3);
            assert_eq!(
                rectangles(&galley, 0..text.chars().count()).len(),
                galley.rows.len()
            );
            let data = super::super::interdiff_data(&[format!("+{text}"), "+short".into()]);
            let g = geometry(ui, &data, false, 280.0, &crate::theme::mono(12.0));
            assert!(g.height(0) > LINE_H * 3.0);
            assert_eq!(g.row(g.at(1) + 1.0), 1);
            assert_eq!(g.offsets.len(), data.lines.len() + 1);
        });
        out.textures_delta.clear();
    }
}
