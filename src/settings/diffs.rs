//! The Diffs card of the Git page: folding generated files and lockfiles, and the diff
//! algorithms chosen per file type.

use super::layout::{self, Card};
use super::{FIELD_WIDTH, Settings};

impl Settings {
    pub(super) fn diffs_ui(&mut self, card: &mut Card) {
        card.row(
            "Fold generated files",
            "lockfiles minified linguist-generated gitattributes collapse",
            |ui| {
                ui.checkbox(
                    &mut self.fold_generated,
                    crate::i18n::text("interaction-settings-generated"),
                )
                .on_hover_text(crate::i18n::text("interaction-settings-generated-tip"));
            },
        );
        card.row_hint(
            "Also fold",
            crate::i18n::text("interaction-settings-fold-hint"),
            "fold patterns glob generated",
            |ui| {
                ui.add_enabled(
                    self.fold_generated,
                    layout::field(&mut self.fold_patterns)
                        .hint_text("*.snap, dist/**")
                        .desired_width(layout::fit(ui, FIELD_WIDTH)),
                )
                .on_hover_text(crate::i18n::text("interaction-settings-fold-pattern-tip"));
            },
        );
        card.row(
            "Diff algorithms",
            "myers minimal patience histogram algorithm file type",
            |ui| {
                if self.diff_algorithms.is_empty() {
                    layout::note(ui, crate::i18n::text("interaction-settings-diff-default"));
                    return;
                }
                ui.vertical(|ui| {
                    let mut forget = None;
                    for (kind, algorithm) in &self.diff_algorithms {
                        ui.horizontal(|ui| {
                            ui.label(format!("{kind}: {}", algorithm.label()));
                            if ui
                                .small_button(crate::i18n::text("interaction-settings-forget"))
                                .clicked()
                            {
                                forget = Some(kind.clone());
                            }
                        });
                    }
                    if let Some(kind) = forget {
                        self.diff_algorithms.remove(&kind);
                    }
                    layout::note(ui, crate::i18n::text("interaction-settings-diff-note"));
                });
            },
        );
    }
}
