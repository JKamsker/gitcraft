//! Resource details extend the selected worktree panel; all controls are read-only.
use super::State;
use crate::git::worktree_resources::Completeness;
use crate::i18n::text;
use crate::repo::Action;
use crate::repo::insights::constellation::{Act, act};

pub(in crate::repo::insights::constellation) fn show(
    ui: &mut egui::Ui,
    state: &mut State,
    path: &str,
    acts: &mut Vec<Action>,
) {
    ui.separator();
    ui.horizontal(|ui| {
        if ui.button(text("resources-refresh")).clicked() {
            acts.push(act(Act::Measure(path.into())));
        }
        if state.result.loading && ui.button(text("resources-stop")).clicked() {
            acts.push(act(Act::StopResources));
        }
    });
    ui.label(text("resources-observation"));
    if state.path != path {
        ui.label(text("resources-not-measured"));
        return;
    }
    if state.result.loading {
        ui.add(crate::spinner::Spinner::new());
    }
    if let Some(error) = state.result.error() {
        ui.label(error);
    }
    let Some(found) = state.result.ready() else {
        return;
    };
    ui.monospace(found.root.to_string_lossy());
    ui.label(format!(
        "HEAD {} · {} · #{}",
        found.head,
        crate::git::relative_time(found.collected, crate::git::now()),
        found.generation
    ));
    ui.label(format!("{}: {:?}", text("resources-branch"), found.branch));
    ui.label(format!(
        "{}: {}/{}/{}/{}",
        text("resources-status"),
        found.staged,
        found.unstaged,
        found.conflicted,
        found.untracked
    ));
    ui.label(crate::i18n::message(&found.publication, &[]));
    if let Some(presence) = &found.presence {
        ui.label(presence);
    }
    ui.label(text("resources-external-unknown"));
    egui::Grid::new("resource-measurements").show(ui, |ui| {
        for (key, m) in [
            "resources-tracked",
            "resources-untracked",
            "resources-build",
            "resources-ignored",
            "resources-unclassified",
        ]
        .iter()
        .zip(&found.measurements)
        {
            ui.label(text(key));
            let value = crate::insights::constellation::read::human_bytes(m.bytes);
            ui.label(match &m.status {
                Completeness::Complete => format!("{value} · {}", m.files),
                Completeness::Partial(why) => {
                    format!(
                        "≥ {value} · {}: {}",
                        text("resources-partial"),
                        crate::i18n::message(why, &[])
                    )
                }
                Completeness::Unavailable(why) => {
                    format!(
                        "{}: {}",
                        text("resources-unavailable"),
                        crate::i18n::message(why, &[])
                    )
                }
                #[cfg(any(not(target_os = "linux"), test))]
                Completeness::Unsupported => text("resources-platform").into(),
            });
            ui.end_row();
        }
    });
    if ui.button(text("resources-preview")).clicked() {
        acts.push(act(Act::Preview(path.into())));
    }
    if state.validation.loading {
        ui.add(crate::spinner::Spinner::new());
    }
    if state.preview {
        if state
            .checked
            .is_some_and(|at| at.elapsed() < std::time::Duration::from_secs(1))
        {
            super::preview::show(ui, found);
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_secs(1));
        } else {
            acts.push(act(Act::Preview(path.into())));
        }
    }
}
