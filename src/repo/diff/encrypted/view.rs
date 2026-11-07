use super::{Comparison, State};
use crate::theme::{self, Tokens};

pub(super) fn show(ui: &mut egui::Ui, state: &State) {
    let t = Tokens::get(ui.ctx());
    ui.label(egui::RichText::new("Private decrypted comparison · read only").color(t.warning));
    ui.label("Plaintext is held only by this view. Closing, refreshing, changing modes or leaving this tab discards it. Return to the encrypted patch to stage changes.");
    match &state.result {
        None => {
            ui.add(crate::spinner::Spinner::new());
            ui.label("Decrypting with this repository machine's SOPS and keys… Cancel returns to the encrypted patch.");
        }
        Some(Err(reason)) => {
            ui.colored_label(t.warning, reason);
            ui.label("Choose Encrypted patch to return, or Show decrypted again to retry.");
        }
        Some(Ok(data)) => comparison(ui, data, &t),
    }
}

fn comparison(ui: &mut egui::Ui, data: &Comparison, t: &Tokens) {
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
    egui::ScrollArea::both()
        .id_salt("private_sops_diff")
        .auto_shrink([false, false])
        .show_rows(ui, 19.0, data.rows.len(), |ui, range| {
            for row in &data.rows[range] {
                let (mark, color, fill) = match row.kind {
                    similar::ChangeTag::Equal => (" ", t.text, t.dock),
                    similar::ChangeTag::Delete => ("−", t.danger, t.diff_del_bg),
                    similar::ChangeTag::Insert => ("+", t.success, t.diff_add_bg),
                };
                // Bypass egui's shared GalleyCache for private text. The galley belongs
                // only to this paint frame; glyph atlases contain individual characters.
                let galley = private_galley(ui, row.text.replace('\t', "    "), color);
                let (rect, _) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width().max(110.0 + galley.size().x), 19.0),
                    egui::Sense::hover(),
                );
                ui.painter().rect_filled(rect, 0.0, fill);
                ui.painter().text(
                    rect.min + egui::vec2(4.0, 2.0),
                    egui::Align2::LEFT_TOP,
                    format!(
                        "{:>5} {:>5} {mark}",
                        row.old.map_or(String::new(), |n| n.to_string()),
                        row.new.map_or(String::new(), |n| n.to_string())
                    ),
                    theme::mono(11.0),
                    t.text_faint,
                );
                ui.painter()
                    .galley(rect.min + egui::vec2(108.0, 2.0), galley, color);
            }
        });
}

/// Unlike FontsView::layout_job (and Label), raw text::layout does not memoize text.
pub(super) fn private_galley(
    ui: &egui::Ui,
    text: String,
    color: egui::Color32,
) -> std::sync::Arc<egui::Galley> {
    let job = egui::text::LayoutJob::simple_singleline(text, theme::mono(12.0), color);
    let pixels_per_point = ui.ctx().pixels_per_point();
    ui.fonts_mut(|fonts| {
        std::sync::Arc::new(egui::epaint::text::layout(
            fonts.fonts,
            pixels_per_point,
            std::sync::Arc::new(job),
        ))
    })
}
