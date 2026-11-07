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
                ui.checkbox(&mut self.fold_generated, "Start them folded")
                    .on_hover_text(
                        "Files marked linguist-generated or -diff in .gitattributes, lockfiles \
                         (Cargo.lock, package-lock.json, yarn.lock…) and minified files start \
                         folded in file lists and diffs. One click opens them.",
                    );
            },
        );
        card.row_hint(
            "Also fold",
            "Glob patterns of your own",
            "fold patterns glob generated",
            |ui| {
                ui.add_enabled(
                    self.fold_generated,
                    layout::field(&mut self.fold_patterns)
                        .hint_text("*.snap, dist/**")
                        .desired_width(layout::fit(ui, FIELD_WIDTH)),
                )
                .on_hover_text(
                    "Separated by commas or spaces. A pattern without a / matches the file's \
                     name; one with a / its whole path, where * stays within a folder and ** \
                     goes through folders; one ending in / everything in that folder.",
                );
            },
        );
        card.row(
            "Diff algorithms",
            "myers minimal patience histogram algorithm file type",
            |ui| {
                if self.diff_algorithms.is_empty() {
                    layout::note(
                        ui,
                        "git's default for every file type; pick one in a diff's header",
                    );
                    return;
                }
                ui.vertical(|ui| {
                    let mut forget = None;
                    for (kind, algorithm) in &self.diff_algorithms {
                        ui.horizontal(|ui| {
                            ui.label(format!("{kind}: {}", algorithm.label()));
                            if ui.small_button("Forget").clicked() {
                                forget = Some(kind.clone());
                            }
                        });
                    }
                    if let Some(kind) = forget {
                        self.diff_algorithms.remove(&kind);
                    }
                    layout::note(ui, "Diffs read from now on use what is listed");
                });
            },
        );
    }
}
