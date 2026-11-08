use super::{Comparison, State};
use crate::theme::{self, Tokens};

pub(super) fn show(ui: &mut egui::Ui, state: &mut State) {
    let t = Tokens::get(ui.ctx());
    ui.label(egui::RichText::new("Private decrypted comparison · read only").color(t.warning));
    ui.label("Plaintext is held only by this view. Closing, refreshing, changing modes or leaving this tab discards it. Return to the encrypted patch to stage changes.");
    match &state.result {
        None => {
            ui.add(crate::spinner::Spinner::new());
            ui.label("Decrypting with local SOPS and keys on this computer… Cancel returns to the encrypted patch.");
        }
        Some(Err(reason)) => {
            ui.colored_label(t.warning, reason);
            ui.label("Choose Encrypted patch to return, or Show decrypted again to retry.");
        }
        Some(Ok(data)) => {
            if ui.selectable_label(state.wrap, "Wrap").clicked() {
                state.wrap = !state.wrap;
            }
            comparison(ui, data, &t, state.wrap, &mut state.wrap_geometry);
        }
    }
}

fn comparison(
    ui: &mut egui::Ui,
    data: &Comparison,
    t: &Tokens,
    wrap: bool,
    geometry: &mut Option<(u32, Vec<f32>)>,
) {
    ui.horizontal_wrapped(|ui| {
        ui.label(if data.content_changed {
            "Decrypted content: changed"
        } else {
            "Decrypted content: unchanged"
        });
        ui.separator();
        ui.label(if data.recipients_changed {
            "Recipients / wrapped keys: changed"
        } else {
            "Recipients / wrapped keys: unchanged"
        });
        if data.metadata_changed && !data.recipients_changed {
            ui.separator();
            ui.label("Other encryption metadata changed");
        }
    });
    if !data.content_changed {
        ui.label("No plaintext changes. Ciphertext or integrity metadata may have changed through re-encryption.");
    }
    ui.separator();
    let width = (ui.available_width() - 124.).max(1.);
    if wrap
        && geometry
            .as_ref()
            .is_none_or(|(key, _)| *key != width.to_bits())
    {
        let mut offsets = Vec::with_capacity(data.rows.len() + 1);
        offsets.push(0.);
        for row in &data.rows {
            let galley = private_wrapped_galley(
                ui,
                row.text.trim_end_matches('\n').replace('\t', "    "),
                t.text,
                Some(width),
            );
            offsets.push(offsets.last().copied().unwrap_or(0.) + (galley.size().y + 4.).max(19.));
        }
        // Numeric row offsets only. No plaintext, layouts or galleys enter shared UI data.
        *geometry = Some((width.to_bits(), offsets));
    }
    let paint = |ui: &mut egui::Ui, i: usize, height: f32| {
        let row = &data.rows[i];
        let (mark, color, fill) = match row.kind {
            super::super::line_diff::Edit::Same => (" ", t.text, t.dock),
            super::super::line_diff::Edit::Del => ("−", t.danger, t.diff_del_bg),
            super::super::line_diff::Edit::Add => ("+", t.success, t.diff_add_bg),
        };
        let galley = private_wrapped_galley(
            ui,
            row.text.trim_end_matches('\n').replace('\t', "    "),
            color,
            wrap.then_some(width),
        );
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(ui.available_width().max(110. + galley.size().x), height),
            egui::Sense::hover(),
        );
        ui.painter().rect_filled(rect, 0., fill);
        ui.painter().text(
            rect.min + egui::vec2(4., 2.),
            egui::Align2::LEFT_TOP,
            format!(
                "{:>5} {:>5} {mark}",
                row.old.map_or(String::new(), |n| n.to_string()),
                row.new.map_or(String::new(), |n| n.to_string())
            ),
            theme::mono(11.),
            t.text_faint,
        );
        ui.painter()
            .galley(rect.min + egui::vec2(108., 2.), galley, color);
    };
    ui.spacing_mut().item_spacing.y = 0.;
    let scroll = if wrap {
        egui::ScrollArea::vertical()
    } else {
        egui::ScrollArea::both()
    }
    .id_salt("private_sops_diff")
    .auto_shrink([false, false]);
    if let Some((_, offsets)) = geometry.as_ref().filter(|_| wrap) {
        scroll.show_viewport(ui, |ui, viewport| {
            ui.set_min_height(*offsets.last().unwrap_or(&0.));
            let start = offsets
                .partition_point(|y| *y <= viewport.top())
                .saturating_sub(2)
                .min(data.rows.len());
            let end =
                (offsets.partition_point(|y| *y <= viewport.bottom()) + 1).min(data.rows.len());
            ui.add_space(offsets[start]);
            for i in start..end {
                paint(ui, i, offsets[i + 1] - offsets[i]);
            }
        });
    } else {
        scroll.show_rows(ui, 19., data.rows.len(), |ui, range| {
            for i in range {
                paint(ui, i, 19.);
            }
        });
    }
}

/// Unlike FontsView::layout_job (and Label), raw text::layout does not memoize text.
#[cfg(test)]
pub(super) fn private_galley(
    ui: &egui::Ui,
    text: String,
    color: egui::Color32,
) -> std::sync::Arc<egui::Galley> {
    private_wrapped_galley(ui, text, color, None)
}

pub(super) fn private_wrapped_galley(
    ui: &egui::Ui,
    text: String,
    color: egui::Color32,
    width: Option<f32>,
) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::simple_singleline(text, theme::mono(12.0), color);
    if let Some(width) = width {
        job.wrap.max_width = width;
        job.wrap.break_anywhere = true;
    }
    let pixels_per_point = ui.ctx().pixels_per_point();
    ui.fonts_mut(|fonts| {
        std::sync::Arc::new(egui::epaint::text::layout(
            fonts.fonts,
            pixels_per_point,
            std::sync::Arc::new(job),
        ))
    })
}
