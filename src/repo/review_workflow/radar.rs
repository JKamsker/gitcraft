//! Patch identity, explicit adaptation, and observed behavior are independent witnesses.
use super::{
    contracts::{self, Observation},
    store,
    tools::ToolsAct,
};
use crate::{
    git,
    repo::{Action, RepoTab},
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path};
const HISTORY: usize = 5000;
#[derive(Clone, Serialize, Deserialize)]
struct Pin {
    name: String,
    fix: String,
    targets: Vec<(String, Option<String>)>,
    command: String,
}
#[derive(Clone, Default)]
pub(crate) struct Form {
    name: String,
    fix: String,
    targets: String,
    command: String,
    trusted: bool,
}
#[derive(Clone, Serialize, Deserialize)]
struct Row {
    name: String,
    fix: String,
    target: String,
    tip: String,
    patch: Patch,
    test: Option<Observation>,
    coverage: usize,
    complete: bool,
    error: Option<String>,
}
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
enum Patch {
    Same(String),
    Adapted(String),
    Reverted(String),
    NotChecked,
}
#[derive(Default)]
pub(crate) struct State {
    form: Form,
    pins: Vec<Pin>,
    rows: Option<Result<Vec<Row>, String>>,
    busy: bool,
    stop: git::RunStop,
}
impl Drop for State {
    fn drop(&mut self) {
        self.stop.stop();
    }
}
#[derive(Clone)]
pub(crate) enum Act {
    Load,
    Pin(Form),
    Check,
    Test { index: usize, trusted: bool },
    Stop,
    Remove(usize),
}
fn patch_id(repo: &Path, rev: &str, reverse: bool) -> Result<String, String> {
    let patch = if reverse {
        let parent =
            git::run(Some(repo), &["rev-parse", "--verify", &format!("{rev}^")]).or_else(|_| {
                git::run_input(Some(repo), &["hash-object", "-t", "tree", "--stdin"], "")
            })?;
        git::run(
            Some(repo),
            &[
                "diff",
                "--src-prefix=a/",
                "--dst-prefix=b/",
                "--no-ext-diff",
                "--no-textconv",
                "--no-renames",
                rev,
                parent.trim(),
                "--",
            ],
        )?
    } else {
        git::run(
            Some(repo),
            &[
                "show",
                "--format=medium",
                "--src-prefix=a/",
                "--dst-prefix=b/",
                "--no-ext-diff",
                "--no-textconv",
                "--no-renames",
                rev,
                "--",
            ],
        )?
    };
    let out = git::run_input(Some(repo), &["patch-id", "--stable"], &patch)?;
    out.split_whitespace()
        .next()
        .map(str::to_owned)
        .ok_or("Pin a nonempty, nonmerge fix patch".into())
}
type HistoryEvidence = (Vec<(String, String)>, usize, bool);
impl RepoTab {
    pub(in crate::repo) fn radar_act(&mut self, act: Act, ctx: &egui::Context) {
        if matches!(act, Act::Stop) {
            self.review_tools.radar.stop.stop();
            return;
        }
        if self.review_tools.radar.busy {
            return;
        }
        self.review_tools.radar.busy = true;
        self.review_tools.radar.stop = git::RunStop::default();
        let (repo, stop) = (self.path.clone(), self.review_tools.radar.stop.clone());
        self.then_or(
            ctx,
            true,
            Err,
            move || {
                let mut pins =
                    store::load::<Vec<Pin>>(&repo, "fix-radar", "pins")?.unwrap_or_default();
                match act {
                    Act::Pin(form) => {
                        let fix = contracts::resolve(&repo, &form.fix)?;
                        patch_id(&repo, &fix, false)?;
                        let mut targets = vec![];
                        for line in form
                            .targets
                            .lines()
                            .map(str::trim)
                            .filter(|l| !l.is_empty())
                        {
                            let (target, adaptation) = line
                                .split_once('=')
                                .map_or((line, None), |(t, a)| (t.trim(), Some(a.trim())));
                            contracts::resolve(&repo, target)?;
                            targets.push((
                                target.into(),
                                adaptation
                                    .map(|a| contracts::resolve(&repo, a))
                                    .transpose()?,
                            ));
                        }
                        if targets.is_empty() || targets.len() > 32 {
                            return Err("Choose 1–32 release targets".into());
                        }
                        git::run(
                            Some(&repo),
                            &[
                                "update-ref",
                                &format!("refs/gitcraft/pinned-fixes/{fix}"),
                                &fix,
                            ],
                        )?;
                        for (_, adapted) in &targets {
                            if let Some(adapted) = adapted {
                                git::run(
                                    Some(&repo),
                                    &[
                                        "update-ref",
                                        &format!("refs/gitcraft/pinned-fixes/{adapted}"),
                                        adapted,
                                    ],
                                )?;
                            }
                        }
                        pins.push(Pin {
                            name: form.name,
                            fix,
                            targets,
                            command: form.command,
                        });
                        store::save(&repo, "fix-radar", "pins", &pins)?;
                    }
                    Act::Remove(index) => {
                        if index >= pins.len() {
                            return Err("Reload the pinned fixes".into());
                        }
                        pins.remove(index);
                        store::save(&repo, "fix-radar", "pins", &pins)?;
                    }
                    Act::Test { index, trusted } => {
                        if !trusted {
                            return Err(
                                "Explicitly trust the regression command before running it".into(),
                            );
                        }
                        test(&repo, pins.get(index).ok_or("Reload pinned fixes")?, &stop)?;
                    }
                    _ => {}
                }
                let rows = check(&repo, &pins)?;
                Ok((pins, rows))
            },
            |tab, result, _| {
                let s = &mut tab.review_tools.radar;
                s.busy = false;
                match result {
                    Ok((pins, rows)) => {
                        s.pins = pins;
                        s.rows = Some(Ok(rows));
                    }
                    Err(e) => s.rows = Some(Err(e)),
                }
            },
        );
    }
}
#[cfg(test)]
#[path = "radar/fluent_review_tests.rs"]
mod fluent_review_tests;
#[cfg(test)]
#[path = "radar/tests.rs"]
mod tests;

/// Only this application-owned action supplies fixed-caption provenance.
fn adapted_fix_comparison(original: &str, adapted: &str) -> Action {
    Action::Compare {
        base: original.to_owned(),
        base_label: "Original fix".into(),
        target: Some(adapted.to_owned()),
        target_label: "Adapted fix".into(),
        captions: [
            Some(crate::repo::state::CompareCaption::OriginalFix),
            Some(crate::repo::state::CompareCaption::AdaptedFix),
        ],
        since_fork: false,
    }
}

#[cfg(test)]
#[path = "radar/comparison_caption_tests.rs"]
mod comparison_caption_tests;

mod evidence;
use evidence::{check, test};
#[cfg(test)]
use evidence::{history, patch_status, status};

mod presentation;
pub(super) use presentation::show;
