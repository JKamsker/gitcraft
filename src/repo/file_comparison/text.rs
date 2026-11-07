//! Full, virtualized raw comparison text. Wrapped rows retain their original patch lines.
use super::model::Boundary;
use crate::{repo::diff::Kind, theme::Tokens};
use std::{
    hash::{Hash, Hasher},
    sync::Arc,
};

#[derive(Clone)]
struct Layout {
    offsets: Vec<f32>,
    width: f32,
}
fn galley(
    ui: &egui::Ui,
    text: &str,
    color: egui::Color32,
    width: Option<f32>,
) -> Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::simple_singleline(
        text.replace('\t', "    "),
        crate::theme::mono(12.),
        color,
    );
    if let Some(width) = width {
        job.wrap.max_width = width.max(1.);
        job.wrap.break_anywhere = true;
    }
    ui.fonts_mut(|f| f.layout_job(job))
}
fn layout(ui: &egui::Ui, part: &Boundary, width: Option<f32>) -> Arc<Layout> {
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    part.full.hash(&mut hash);
    let key = (
        hash.finish(),
        width.map(f32::to_bits),
        ui.ctx().pixels_per_point().to_bits(),
    );
    let id = ui.id().with("content-comparison-geometry");
    if let Some((previous, geometry)) = ui
        .ctx()
        .data(|d| d.get_temp::<((u64, Option<u32>, u32), Arc<Layout>)>(id))
        && previous == key
    {
        return geometry;
    }
    let mut offsets = vec![0.];
    let mut longest = 0f32;
    for line in &part.lines {
        let g = galley(ui, &line.raw, egui::Color32::WHITE, width);
        offsets.push(offsets.last().copied().unwrap_or(0.) + (g.size().y + 4.).max(20.));
        longest = longest.max(g.size().x + 12.);
    }
    let geometry = Arc::new(Layout {
        offsets,
        width: longest,
    });
    ui.ctx()
        .data_mut(|d| d.insert_temp(id, (key, geometry.clone())));
    geometry
}
pub(super) fn show(ui: &mut egui::Ui, part: &Boundary, t: &Tokens) {
    let id = ui.id().with("content-comparison-wrap");
    let mut wrap = ui.ctx().data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
    if ui.selectable_label(wrap, "Wrap").clicked() {
        wrap = !wrap;
        ui.ctx().data_mut(|d| d.insert_temp(id, wrap));
    }
    let width = wrap.then(|| (ui.available_width() - 24.).max(1.));
    let geometry = layout(ui, part, width);
    ui.spacing_mut().item_spacing.y = 0.;
    let scroll = if wrap {
        egui::ScrollArea::vertical()
    } else {
        egui::ScrollArea::both()
    }
    .id_salt("snapshot_content_diff");
    scroll.show_viewport(ui, |ui, viewport| {
        ui.set_min_height(*geometry.offsets.last().unwrap_or(&0.));
        if !wrap {
            ui.set_min_width(geometry.width);
        }
        let start = geometry
            .offsets
            .partition_point(|y| *y <= viewport.top())
            .saturating_sub(2)
            .min(part.lines.len());
        let end = (geometry
            .offsets
            .partition_point(|y| *y <= viewport.bottom())
            + 1)
        .min(part.lines.len());
        ui.add_space(geometry.offsets[start]);
        for i in start..end {
            let line = &part.lines[i];
            let (color, fill) = match line.kind {
                Kind::Add => (t.success, t.diff_add_bg),
                Kind::Del => (t.danger, t.diff_del_bg),
                _ => (t.text, t.card),
            };
            let galley = galley(ui, &line.raw, color, width);
            let (rect, _) = ui.allocate_exact_size(
                egui::vec2(
                    ui.available_width().max(galley.size().x + 12.),
                    geometry.offsets[i + 1] - geometry.offsets[i],
                ),
                egui::Sense::hover(),
            );
            ui.painter().rect_filled(rect, 0., fill);
            ui.painter()
                .galley(rect.min + egui::vec2(6., 2.), galley, color);
        }
    });
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn full_long_unicode_lines_are_available_in_wrapped_and_unwrapped_layouts() {
        let ctx = egui::Context::default();
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            let text = format!("{}尾", "long_αβγ_argument ".repeat(400));
            let plain = galley(ui, &text, egui::Color32::WHITE, None);
            let wrapped = galley(ui, &text, egui::Color32::WHITE, Some(160.));
            assert_eq!(plain.text(), text);
            assert_eq!(wrapped.text(), text);
            assert!(wrapped.rows.len() > 10);
            assert!(plain.size().x > wrapped.size().x * 10.);
        });
        out.textures_delta.clear();
    }
}
