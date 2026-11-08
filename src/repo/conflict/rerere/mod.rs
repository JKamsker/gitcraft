//! Remembered conflict resolutions (`git rerere`): the repository's switch, and a conflicted
//! file that rerere already resolved, opened in the conflict editor with what it reused.

mod commands;

use std::collections::BTreeSet;
use std::path::Path;

pub use self::commands::{Setup, forget_ops, reused_in, setup};
use super::{Choice, ConflictEditor, Reused, parse, render, split_resolutions};
use crate::git::{self, InProgress};
use crate::repo::RepoTab;

/// What the tab knows of rerere in its repository.
#[derive(Default)]
pub struct Rerere {
    /// How rerere is set up here (`None` until read).
    pub setup: Option<Setup>,
    /// Being read, or turned on or off: the switch waits.
    pub busy: bool,
    /// The files the last operation said it resolved from a recorded resolution.
    pub reused: BTreeSet<String>,
    /// Conflicted files without markers that couldn't be rebuilt since the last operation:
    /// not tried again on every refresh.
    pub unopenable: BTreeSet<String>,
}

impl Rerere {
    /// Whether rerere is on here (`None` until read).
    pub fn enabled(&self) -> Option<bool> {
        self.setup.map(|s| s.on)
    }
}

/// What the views ask about remembered resolutions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RerereAct {
    /// Turn rerere on or off for this repository.
    SetEnabled(bool),
    /// Have it leave what it reuses unstaged here.
    LeaveUnstaged,
}

/// How many files git's output says were resolved from a recorded resolution.
pub fn reused_count(output: &str) -> usize {
    reused_in(output).len()
}

impl ConflictEditor {
    /// The editor for conflicted `path`, whose conflict markers are gone (`current`: the file
    /// now) though git still has it unmerged: rerere reused a resolution (`certain` when git
    /// said so), or it was edited elsewhere. Each conflict, rebuilt from `preimage`, shows the
    /// text that now stands in its place.
    pub fn reused(
        path: &str,
        preimage: &str,
        current: String,
        state: Option<InProgress>,
        certain: bool,
    ) -> Option<Self> {
        let (segments, ours_label, theirs_label) = parse(preimage);
        let conflicts = segments
            .iter()
            .filter(|s| matches!(s, super::Segment::Conflict { .. }))
            .count();
        if conflicts == 0 {
            return None;
        }
        let split = split_resolutions(&segments, &current);
        let (choices, recorded) = match &split {
            Some(texts) => (
                vec![Some(Choice::Recorded); conflicts],
                texts.iter().cloned().map(Some).collect(),
            ),
            None => (vec![None; conflicts], vec![None; conflicts]),
        };
        let mut ed = Self {
            original: Default::default(),
            history: Default::default(),
            path: path.to_owned(),
            segments,
            choices,
            recorded,
            result: current.clone(),
            manual: false,
            ours_label,
            theirs_label,
            rebase: state == Some(InProgress::Rebase),
            solving: false,
            on_disk: 0,
            opened: current,
            generation: 0,
            saving: false,
            reused: Some(if certain {
                Reused::Rerere
            } else {
                Reused::NoMarkers
            }),
            memo: Default::default(),
        };
        // What can't be told apart conflict by conflict stays as it is, edited by hand.
        let labels = (ed.ours_label.as_str(), ed.theirs_label.as_str());
        ed.manual =
            split.is_none() || render(&ed.segments, &ed.choices, &ed.recorded, labels) != ed.result;
        Some(ed)
    }
}

/// Opens conflicted `path` (in the repository at `repo`) when its markers are gone, with what
/// stands in their place (see [`ConflictEditor::reused`]). Blocking: from the background.
pub fn open_reused(
    repo: &Path,
    path: &str,
    state: Option<InProgress>,
    certain: bool,
) -> Result<Option<ConflictEditor>, String> {
    let file = crate::remote::join(repo, path);
    if super::is_link(&file) {
        return Ok(None);
    }
    let Ok(bytes) = git::worktree_file(&file) else {
        return Ok(None);
    };
    let Ok(current) = String::from_utf8(bytes) else {
        return Ok(None);
    };
    let Some(before) = commands::preimage(repo, path, state, &current)? else {
        return Ok(None);
    };
    Ok(ConflictEditor::reused(
        path, &before, current, state, certain,
    ))
}

impl RepoTab {
    /// Reads how rerere is set up here, in the background (once at a time).
    pub(in crate::repo) fn read_rerere(&mut self, ctx: &egui::Context) {
        if self.rerere.busy {
            return;
        }
        self.rerere.busy = true;
        let repo = self.path.clone();
        self.then(
            ctx,
            move || setup(&repo),
            |tab, read, _| {
                tab.rerere.busy = false;
                match read {
                    Ok(read) => tab.rerere.setup = Some(read),
                    // Not asked again on every reload: the switch says it.
                    Err(e) => {
                        tab.rerere.setup = Some(Setup::default());
                        tab.status = Some((crate::i18n::message("repo-core-0671", &[("e", format!("{}", e).into())]), true));
                    }
                }
            },
        );
    }

    /// Opens conflicted `path` without its markers in the background (see [`open_reused`]),
    /// for the opening numbered `generation`; not again when it couldn't be since the last
    /// operation.
    pub(in crate::repo) fn open_reused_later(
        &self,
        generation: u64,
        path: String,
        ctx: &egui::Context,
    ) {
        if self.rerere.unopenable.contains(&path) {
            return;
        }
        let (repo, state) = (self.path.clone(), self.work.in_progress);
        let certain = self.rerere.reused.contains(&path);
        self.then(
            ctx,
            {
                let path = path.clone();
                move || open_reused(&repo, &path, state, certain)
            },
            move |tab, found, _| tab.reused_opened(generation, path, found),
        );
    }

    /// Takes the editor for a file without its markers, read for the opening numbered
    /// `generation` (see [`Self::conflict_opened`]). One that couldn't be opened isn't tried
    /// again before the next operation; why is said once, while that opening is the latest.
    pub(in crate::repo) fn reused_opened(
        &mut self,
        generation: u64,
        path: String,
        found: Result<Option<ConflictEditor>, String>,
    ) {
        match found {
            Ok(Some(editor)) => self.conflict_opened(generation, path, Some(Box::new(editor))),
            Ok(None) => {
                self.rerere.unopenable.insert(path);
            }
            Err(e) => {
                if generation == self.conflict_generation {
                    self.status = Some((e, true));
                }
                self.rerere.unopenable.insert(path);
            }
        }
    }

    /// After an operation (`output`: what git said when it failed): which files it resolved
    /// from what was recorded, and files without markers are tried again.
    pub(in crate::repo) fn note_reused(&mut self, output: Option<&str>) {
        self.rerere.unopenable.clear();
        if let Some(output) = output.filter(|o| o.contains("CONFLICT")) {
            self.rerere.reused = reused_in(output);
        }
    }

    pub(in crate::repo) fn rerere_act(&mut self, act: RerereAct, ctx: &egui::Context) {
        if self.rerere.busy {
            return;
        }
        self.rerere.busy = true;
        let repo = self.path.clone();
        let failed = match act {
            RerereAct::SetEnabled(true) => crate::i18n::text("repo-core-0673"),
            RerereAct::SetEnabled(false) => crate::i18n::text("repo-core-0674"),
            RerereAct::LeaveUnstaged => crate::i18n::text("repo-core-0675"),
        };
        self.then(
            ctx,
            move || {
                match act {
                    RerereAct::SetEnabled(on) => commands::switch(&repo, on)?,
                    RerereAct::LeaveUnstaged => commands::leave_unstaged(&repo)?,
                }
                setup(&repo)
            },
            move |tab, now, _| {
                tab.rerere.busy = false;
                tab.status = Some(match now {
                    Ok(now) => {
                        tab.rerere.setup = Some(now);
                        (switched_message(now).to_owned(), false)
                    }
                    Err(e) => (format!("{failed}: {e}"), true),
                });
            },
        );
    }
}

/// What the toolbar says once rerere was set up so.
fn switched_message(now: Setup) -> &'static str {
    match (now.on, now.stages) {
        (true, false) => {
            crate::i18n::text("repo-core-0677")
        }
        (true, true) => {
            crate::i18n::text("repo-core-0678")
        }
        (false, _) => crate::i18n::text("repo-core-0679"),
    }
}

#[cfg(test)]
mod tests;
