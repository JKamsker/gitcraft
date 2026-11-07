//! Choosing the diff algorithm in the diff header: kept per file type in the settings, passed
//! to git for each diff read (the repository's config is never written), and switched without
//! losing one's place.

use egui::Ui;

use super::place::place_at;
use crate::git::{DiffAlgorithm, file_type};
use crate::repo::{Action, RepoTab};
use crate::settings::Settings;

/// The algorithm the settings keep for the file at `path` (`None`: git's default).
pub(in crate::repo) fn algorithm_for(settings: &Settings, path: &str) -> Option<DiffAlgorithm> {
    settings.diff_algorithms.get(&file_type(path)).copied()
}

/// Keeps `algorithm` for the file type of `path` (`None`: back to git's default). True when
/// that changed what the type uses.
pub(in crate::repo) fn keep_algorithm(
    settings: &mut Settings,
    path: &str,
    algorithm: Option<DiffAlgorithm>,
) -> bool {
    let kind = file_type(path);
    let before = settings.diff_algorithms.get(&kind).copied();
    match algorithm {
        Some(a) => settings.diff_algorithms.insert(kind, a),
        None => settings.diff_algorithms.remove(&kind),
    };
    before != algorithm
}

/// What the drop-down shows for a file type that uses `algorithm` (`None`: git's default).
fn shown_label(algorithm: Option<DiffAlgorithm>) -> &'static str {
    algorithm.map_or("Algorithm", DiffAlgorithm::label)
}

/// The algorithm drop-down of the diff header for the file at `path`.
pub(in crate::repo) fn algorithm_menu(ui: &mut Ui, path: &str, acts: &mut Vec<Action>) {
    let current = algorithm_for(&Settings::get(ui.ctx()), path);
    let kind = file_type(path);
    let name = path
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or(path);
    let kept_for = match kind.strip_prefix('*') {
        Some(ext) => format!("{ext} files"),
        None => format!("files named {name}"),
    };
    let mut picked = current;
    egui::ComboBox::from_id_salt(("diff-algorithm", path))
        .selected_text(shown_label(current))
        .width(92.0)
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut picked, None, "Default")
                .on_hover_text("What your git config says (diff.algorithm), else Myers");
            for a in DiffAlgorithm::ALL {
                ui.selectable_value(&mut picked, Some(a), a.label())
                    .on_hover_text(a.about());
            }
        })
        .response
        .on_hover_text(format!(
            "Diff algorithm: how the lines of both sides are paired. Kept for {kept_for}; \
             the repository's config is left as it is."
        ));
    if picked != current {
        acts.push(Action::SetDiffAlgorithm(picked));
    }
}

impl RepoTab {
    /// Uses `algorithm` for the open diff's file type from now on, and reads the diff again
    /// with it (also when that was already the choice but the diff was read another way),
    /// keeping the line at the top of the view at the top.
    pub(in crate::repo) fn set_diff_algorithm(
        &mut self,
        algorithm: Option<DiffAlgorithm>,
        ctx: &egui::Context,
    ) {
        let mut settings = Settings::get(ctx);
        let Some(view) = &mut self.diff else { return };
        let changed = keep_algorithm(&mut settings, view.key.path(), algorithm);
        // Unchanged, but this diff was read another way (changed in another tab, or forgotten
        // in the settings): read again all the same.
        if !changed && view.algorithm == algorithm {
            return;
        }
        if changed {
            settings.set(ctx);
        }
        if let Some(Ok(data)) = &view.data {
            view.keep_place = place_at(data, data.shows_split(settings.diff_split), view.scroll);
        }
        self.refresh_diff(ctx);
    }
}
