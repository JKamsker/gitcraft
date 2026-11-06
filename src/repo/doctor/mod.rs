//! The repository doctor, in the big area: a page of sections that explain why git behaves as
//! it does here (which setting wins, which tracked files it was told not to look at) and fix
//! what was set by mistake. Everything is read in the background; changes run as the tab's
//! operations, and the section is read again once they are done.

mod config_section;
mod flags_section;
mod page;

use std::path::PathBuf;

use super::{Action, Refresh, RepoTab};
use crate::git::doctor::config::{self, Origin, Scope};
use crate::git::doctor::flags::{self, Flagged, Sparse};

/// The doctor's sections, in the order they are listed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Section {
    Config,
    Flags,
}

impl Section {
    pub(crate) const ALL: [Section; 2] = [Section::Config, Section::Flags];

    pub(crate) fn title(self) -> &'static str {
        match self {
            Section::Config => "Which configuration wins",
            Section::Flags => "Hidden changes",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Section::Config => "settings",
            Section::Flags => "eye-off",
        }
    }
}

/// What the doctor's page asks for.
pub(crate) enum DoctorAct {
    Show(Section),
    Reload,
    /// Run git commands as one of the tab's operations, then read the section again.
    Run {
        label: String,
        ops: Vec<Vec<String>>,
    },
    /// Read what a setting is in every worktree of the repository.
    CompareWorktrees(&'static str),
}

/// Something read in the background, `None` while it is read.
type Read<T> = Option<Result<T, String>>;

/// What a setting is in each worktree: its folder, and the value in effect there.
type InWorktrees = Vec<(PathBuf, Result<Option<Origin>, String>)>;

/// What the configuration section shows.
#[derive(Default)]
pub(crate) struct ConfigState {
    origins: Read<Vec<Origin>>,
    /// The setting opened, by key.
    open: Option<&'static str>,
    /// Lists settings no file sets, too.
    show_unset: bool,
    filter: String,
    /// The scope and value being written for the setting opened.
    scope: Option<Scope>,
    value: String,
    /// What the opened setting is in each worktree: (key, per worktree).
    worktrees: Option<(&'static str, Read<InWorktrees>)>,
}

/// What the hidden-changes section shows.
#[derive(Default)]
pub(crate) struct FlagsState {
    list: Read<(Vec<Flagged>, Sparse)>,
    /// The manual flags ticked to be cleared, by path.
    ticked: std::collections::BTreeSet<String>,
}

pub(crate) struct DoctorView {
    pub(crate) section: Section,
    /// Numbers the reads, so an answer for an older one is dropped.
    read: u64,
    /// An operation of the doctor's is running: read the section again once the tab is idle.
    waiting: bool,
    config: ConfigState,
    flags: FlagsState,
}

impl RepoTab {
    /// Shows the doctor in the big area, at `section`.
    pub(super) fn open_doctor(&mut self, section: Section, ctx: &egui::Context) {
        if self.conflict.is_some() || self.rebase.is_some() {
            self.status = Some((
                "Finish or close the conflict editor or the interactive rebase first".to_owned(),
                true,
            ));
            return;
        }
        self.diff = None;
        self.file_view = None;
        self.ci_view = None;
        self.close_pr();
        let doctor = self.doctor.get_or_insert_with(|| {
            Box::new(DoctorView {
                section,
                read: 0,
                waiting: false,
                config: ConfigState::default(),
                flags: FlagsState::default(),
            })
        });
        doctor.section = section;
        self.load_doctor(ctx);
    }

    pub(super) fn doctor_act(&mut self, act: DoctorAct, ctx: &egui::Context) {
        match act {
            DoctorAct::Show(section) => self.open_doctor(section, ctx),
            DoctorAct::Reload => self.load_doctor(ctx),
            DoctorAct::Run { label, ops } => {
                if self.busy_op.is_some() {
                    self.status = Some(("Wait for the running operation to end".to_owned(), true));
                    return;
                }
                if let Some(doctor) = &mut self.doctor {
                    doctor.waiting = true;
                }
                self.apply(
                    Action::Ops {
                        label,
                        ops,
                        refresh: Refresh::All,
                    },
                    ctx,
                );
            }
            DoctorAct::CompareWorktrees(key) => self.compare_worktrees(key, ctx),
        }
    }

    /// Once the doctor's operation is done, what it changed is read again.
    pub(super) fn doctor_poll(&mut self, ctx: &egui::Context) {
        let done = self
            .doctor
            .as_ref()
            .is_some_and(|d| d.waiting && self.busy_op.is_none());
        if done {
            if let Some(doctor) = &mut self.doctor {
                doctor.waiting = false;
            }
            self.load_doctor(ctx);
        }
    }

    /// Reads the section shown (again), in the background.
    fn load_doctor(&mut self, ctx: &egui::Context) {
        let path = self.path.clone();
        let Some(doctor) = &mut self.doctor else {
            return;
        };
        doctor.read += 1;
        let read = doctor.read;
        match doctor.section {
            Section::Config => {
                doctor.config.origins = None;
                doctor.config.worktrees = None;
                self.then(
                    ctx,
                    move || config::origins(&path),
                    move |tab, found, _| {
                        if let Some(d) = tab.doctor.as_mut().filter(|d| d.read == read) {
                            d.config.origins = Some(found);
                        }
                    },
                );
            }
            Section::Flags => {
                doctor.flags.list = None;
                self.then(
                    ctx,
                    move || flags::flagged(&path),
                    move |tab, found, _| {
                        if let Some(d) = tab.doctor.as_mut().filter(|d| d.read == read) {
                            // What was ticked and is no longer flagged is let go of.
                            if let Ok((list, _)) = &found {
                                d.flags.ticked.retain(|p| list.iter().any(|f| &f.path == p));
                            }
                            d.flags.list = Some(found);
                        }
                    },
                );
            }
        }
    }

    /// Reads what `key` is in each of the repository's worktrees.
    fn compare_worktrees(&mut self, key: &'static str, ctx: &egui::Context) {
        let dirs: Vec<PathBuf> = self
            .worktrees
            .iter()
            .filter(|w| !w.bare && w.prunable.is_none())
            .map(|w| w.path.clone())
            .collect();
        let Some(doctor) = &mut self.doctor else {
            return;
        };
        doctor.config.worktrees = Some((key, None));
        let read = doctor.read;
        self.then(
            ctx,
            move || config::in_worktrees(&dirs, key),
            move |tab, found, _| {
                if let Some(d) = tab.doctor.as_mut().filter(|d| d.read == read)
                    && let Some((shown, slot)) = &mut d.config.worktrees
                    && *shown == key
                {
                    *slot = Some(Ok(found));
                }
            },
        );
    }
}

#[cfg(test)]
mod tests;
