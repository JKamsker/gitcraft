//! Choosing the diff algorithm in the diff header: kept per file type in the settings, passed
//! to git for each diff read (the repository's config is never written), and switched without
//! losing one's place.

use crate::a11y::Named;
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
    algorithm.map_or(crate::i18n::text("repo-core-1558"), DiffAlgorithm::label)
}

/// The algorithm drop-down of the diff header for the file at `path`, with a spinner beside it
/// while the diff is being read again with it (`repairing`: the lines are still the old ones).
pub(in crate::repo) fn algorithm_menu(
    ui: &mut Ui,
    path: &str,
    repairing: bool,
    acts: &mut Vec<Action>,
) {
    let current = algorithm_for(&Settings::get(ui.ctx()), path);
    let kind = file_type(path);
    let name = path
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or(path);
    let kept_for = match kind.strip_prefix('*') {
        Some(ext) => crate::i18n::message("repo-core-1555", &[("ext", ext.to_string().into())]),
        None => crate::i18n::message("repo-core-1556", &[("name", name.to_string().into())]),
    };
    let mut picked = current;
    let tip = crate::i18n::message(
        "dev-integration-ui-algorithm-tip",
        &[
            ("algorithm", shown_label(current).to_string().into()),
            ("kept_for", kept_for.to_string().into()),
        ],
    );
    // Use egui's submenu-aware menu button: a second root popup would close the
    // responsive overflow menu and make its algorithm choices disappear.
    let tint = crate::theme::Tokens::get(ui.ctx()).icon;
    ui.menu_image_button(crate::icons::image("code", 14.0, tint), |ui| {
        ui.label(crate::i18n::message(
            "dev-integration-ui-algorithm-for",
            &[("kept_for", kept_for.to_string().into())],
        ));
        ui.selectable_value(&mut picked, None, crate::i18n::text("repo-core-1558"))
            .on_hover_text(crate::i18n::text("repo-core-1559"));
        for a in DiffAlgorithm::ALL {
            ui.selectable_value(&mut picked, Some(a), a.label())
                .on_hover_text(a.about());
        }
    })
    .response
    .named(&tip)
    .on_hover_text(tip);
    if repairing {
        // Laid out right to left: beside the drop-down, on its left.
        ui.add(crate::spinner::Spinner::new().size(12.0))
            .on_hover_text(crate::i18n::message(
                "repo-core-1561",
                &[(
                    "label",
                    current
                        .map_or(crate::i18n::text("repo-core-1562"), DiffAlgorithm::label)
                        .to_string()
                        .into(),
                )],
            ));
    }
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
            let logical = super::wrap::logical_offset(
                view.wrap_offsets.as_deref().map(Vec::as_slice),
                view.scroll,
            );
            view.keep_place = place_at(data, data.shows_split(settings.diff_split), logical);
        }
        view.repairing = true;
        self.refresh_diff(ctx);
    }
}
