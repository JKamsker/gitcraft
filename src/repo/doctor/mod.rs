//! The repository doctor, in the big area: a page of sections that explain why git behaves as
//! it does here (why a file is ignored and what attributes it has, which setting wins, which
//! tracked files it was told not to look at) and fix
//! what was set by mistake. Everything is read in the background; changes run as the tab's
//! operations, and the section is read again once they are done.

mod config_section;
mod flags_section;
mod health;
mod health_section;
mod hooks;
mod hooks_section;
mod ignore;
mod ignore_section;
mod page;

use std::collections::HashSet;
use std::path::PathBuf;

use health::HealthState;
pub(crate) use hooks::HooksAct;
use hooks::HooksState;
pub(crate) use ignore::IgnoreAct;
use ignore::IgnoreState;

use super::{Action, Refresh, RepoTab};
use crate::git::doctor::config::{self, Origin, Scope};
use crate::git::doctor::flags::{self, Flag, Flagged, Sparse};

/// The doctor's sections, in the order they are listed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Section {
    Ignore,
    Hooks,
    Health,
    Config,
    Flags,
}

impl Section {
    pub(crate) const ALL: [Section; 5] = [
        Section::Ignore,
        Section::Hooks,
        Section::Health,
        Section::Config,
        Section::Flags,
    ];

    pub(crate) fn title(self) -> &'static str {
        match self {
            Section::Ignore => crate::i18n::text("repo-workflows-doctor-title-94ec061c"),
            Section::Hooks => crate::i18n::text("repo-workflows-doctor-title-d412a062"),
            Section::Health => crate::i18n::text("repo-workflows-doctor-title-8d6acfc3"),
            Section::Config => crate::i18n::text("repo-workflows-doctor-title-8b2d57fa"),
            Section::Flags => crate::i18n::text("repo-workflows-doctor-title-1e6bed5e"),
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Section::Ignore => "eye",
            Section::Hooks => "git-merge",
            Section::Health => "rocket",
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
    /// Ask before running git commands that can't be undone here.
    Confirm(Confirm),
    /// The question asked was answered: run them (`true`) or not.
    Answer(bool),
    /// Read what a setting is in every worktree of the repository.
    CompareWorktrees(&'static str),
    /// Something done in the ignore-and-attributes section.
    Ignore(IgnoreAct),
    /// Something done in the hooks section.
    Hooks(HooksAct),
    /// Close the doctor, asking first when edited rules aren't saved.
    Close,
    /// Keep editing (`false`) or drop the edits and close (`true`).
    AnswerClose(bool),
}

/// A change the doctor asks about before making it.
#[derive(Clone)]
pub(crate) struct Confirm {
    presentation: Option<Box<ConfirmPresentation>>,
    pub(crate) title: String,
    pub(crate) body: String,
    /// The button that makes it.
    pub(crate) button: String,
    pub(crate) label: String,
    pub(crate) ops: Vec<Vec<String>>,
}

/// Something read in the background, `None` while it is read.
type Read<T> = Option<Result<T, String>>;

/// What a setting is in each worktree: its folder, and the value in effect there.
type InWorktrees = Vec<(PathBuf, Result<Option<Origin>, String>)>;

/// The configuration as read.
#[derive(Clone)]
pub(crate) struct ConfigData {
    pub(crate) origins: Vec<Origin>,
    /// `extensions.worktreeConfig` is on: worktrees have files of their own.
    pub(crate) worktree_config: bool,
}

/// The flagged files as read, split once as they come in.
pub(crate) struct FlagsData {
    pub(crate) manual: Vec<Flagged>,
    pub(crate) left_out: Vec<Flagged>,
    pub(crate) sparse: Sparse,
}

/// What the configuration section shows.
#[derive(Default)]
pub(crate) struct ConfigState {
    origins: Read<ConfigData>,
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
    list: Read<FlagsData>,
    /// The manual flags ticked to be cleared, by path.
    ticked: std::collections::BTreeSet<String>,
}

pub(crate) struct DoctorView {
    pub(crate) section: Section,
    /// Numbers the reads, so an answer for an older one is dropped.
    read: u64,
    /// An operation of the doctor's is running: read the section again once the tab is idle.
    waiting: bool,
    /// The change being asked about.
    confirm: Option<Confirm>,
    /// Closing was asked for with rules edited and not saved: asking whether to drop them.
    closing: bool,
    ignore: IgnoreState,
    hooks: HooksState,
    health: HealthState,
    config: ConfigState,
    flags: FlagsState,
}

impl DoctorView {
    /// A change is being asked about, or rules are being edited and not saved: Esc doesn't
    /// close the doctor then.
    pub(crate) fn asking(&self) -> bool {
        self.confirm.is_some() || self.closing || self.ignore.unsaved()
    }
}

impl RepoTab {
    /// Shows the doctor in the big area, at `section`.
    pub(super) fn open_doctor(&mut self, section: Section, ctx: &egui::Context) {
        if self.conflict.is_some() || self.rebase.is_some() {
            self.status = Some(crate::repo::Notice::plain((
                "Finish or close the conflict editor or the interactive rebase first".to_owned(),
                true,
            )));
            return;
        }
        self.diff = None;
        self.file_view = None;
        self.ci_view = None;
        self.close_pr();
        self.tour = None;
        self.insights = None;
        let doctor = self.doctor.get_or_insert_with(|| {
            Box::new(DoctorView {
                section,
                read: 0,
                waiting: false,
                confirm: None,
                closing: false,
                ignore: IgnoreState::default(),
                hooks: HooksState::default(),
                health: HealthState::default(),
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
                    self.status = Some(crate::repo::Notice::plain((
                        "Wait for the running operation to end".to_owned(),
                        true,
                    )));
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
            DoctorAct::Confirm(confirm) => {
                if let Some(doctor) = &mut self.doctor {
                    doctor.confirm = Some(confirm);
                }
            }
            DoctorAct::Answer(yes) => {
                let asked = self.doctor.as_mut().and_then(|d| d.confirm.take());
                if let Some(c) = asked.filter(|_| yes) {
                    self.doctor_act(
                        DoctorAct::Run {
                            label: c.label,
                            ops: c.ops,
                        },
                        ctx,
                    );
                }
            }
            DoctorAct::CompareWorktrees(key) => self.compare_worktrees(key, ctx),
            DoctorAct::Ignore(act) => self.ignore_act(act, ctx),
            DoctorAct::Hooks(act) => self.hooks_act(act, ctx),
            DoctorAct::Close => match &mut self.doctor {
                Some(d) if d.ignore.unsaved() => d.closing = true,
                _ => self.doctor = None,
            },
            DoctorAct::AnswerClose(drop) => {
                if drop {
                    self.doctor = None;
                } else if let Some(d) = &mut self.doctor {
                    d.closing = false;
                }
            }
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
            Section::Ignore => self.load_ignore(read, ctx),
            Section::Hooks => self.load_hooks(read, ctx),
            Section::Health => self.load_health(read, ctx),
            Section::Config => {
                doctor.config.origins = None;
                doctor.config.worktrees = None;
                self.then(
                    ctx,
                    move || {
                        config::origins(&path).map(|origins| ConfigData {
                            origins,
                            worktree_config: config::worktree_config(&path),
                        })
                    },
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
                    move || {
                        flags::flagged(&path).map(|(list, sparse)| {
                            let (manual, left_out) =
                                list.into_iter().partition(|f| f.flag != Flag::Sparse);
                            FlagsData {
                                manual,
                                left_out,
                                sparse,
                            }
                        })
                    },
                    move |tab, found, _| {
                        if let Some(d) = tab.doctor.as_mut().filter(|d| d.read == read) {
                            // What was ticked and is no longer flagged is let go of.
                            if let Ok(data) = &found {
                                let flagged: HashSet<&str> =
                                    data.manual.iter().map(|f| f.path.as_str()).collect();
                                d.flags.ticked.retain(|p| flagged.contains(p.as_str()));
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

#[derive(Clone)]
struct ConfirmPresentation {
    raw: (String, String, String),
    kind: ConfirmKind,
}
#[derive(Clone)]
enum ConfirmKind {
    Remove {
        key: String,
        value: String,
        file: String,
        scope: crate::git::doctor::config::Scope,
    },
    Set {
        key: String,
        value: String,
        scope: crate::git::doctor::config::Scope,
    },
    Clear {
        paths: Vec<String>,
    },
    Maintenance {
        commands: String,
    },
}
impl Confirm {
    fn with_presentation(mut self, kind: ConfirmKind) -> Self {
        self.presentation = Some(Box::new(ConfirmPresentation {
            raw: (self.title.clone(), self.body.clone(), self.button.clone()),
            kind,
        }));
        self
    }
    fn displayed(&self) -> (String, String, String) {
        let raw = (self.title.clone(), self.body.clone(), self.button.clone());
        let Some(receipt) = &self.presentation else {
            return raw;
        };
        if receipt.raw != raw {
            return raw;
        }
        use crate::git::doctor::config::Scope;
        use crate::i18n::{message, text};
        match &receipt.kind {
            ConfirmKind::Remove {
                key,
                value,
                file,
                scope,
            } => (
                message(
                    "repo-workflows-doctor-remove-confirm-9861b6eb",
                    &[("origin-key-1", key.as_str().into())],
                ),
                message(
                    "repo-workflows-doctor-remove-setting",
                    &[
                        ("key", key.as_str().into()),
                        ("value", value.as_str().into()),
                        ("file", file.as_str().into()),
                        (
                            "scope",
                            match scope {
                                Scope::Global => "global",
                                Scope::System => "system",
                                _ => "local",
                            }
                            .into(),
                        ),
                    ],
                ),
                text("repo-workflows-doctor-remove-confirm-c3812fc4").into(),
            ),
            ConfirmKind::Set { key, value, scope } => (
                message(
                    "repo-workflows-doctor-set-row-a215cc6f",
                    &[
                        ("setting-key-1", key.as_str().into()),
                        (
                            "scope-choice-scope-to-lowercase-2",
                            config_section::scope_choice(*scope).to_lowercase().into(),
                        ),
                    ],
                ),
                message(
                    "repo-workflows-doctor-set-row-ccc71902",
                    &[
                        ("setting-key-1", key.as_str().into()),
                        ("value", value.as_str().into()),
                        (
                            "if-scope-scope-global-your-globa-2",
                            text(if *scope == Scope::Global {
                                "repo-workflows-doctor-set-row-e0a790f9"
                            } else {
                                "repo-workflows-doctor-set-row-cc3edd84"
                            })
                            .into(),
                        ),
                    ],
                ),
                text("repo-workflows-doctor-set-row-b6f6f3ad").into(),
            ),
            ConfirmKind::Clear { paths } => {
                let what = match paths.as_slice() {
                    [one] => one.clone(),
                    _ => message(
                        "repo-workflows-doctor-clear-ac70d25c",
                        &[("files-len-1", paths.len().into())],
                    ),
                };
                (
                    text("repo-workflows-doctor-clear-909d7fba").into(),
                    message(
                        "repo-workflows-doctor-clear-ae587a37",
                        &[("what", what.into())],
                    ),
                    text("repo-workflows-doctor-clear-83b12c22").into(),
                )
            }
            ConfirmKind::Maintenance { commands } => (
                text("repo-workflows-doctor-turn-on-action-3c97132f").into(),
                message(
                    "repo-workflows-doctor-turn-on-action-7cde0a5e",
                    &[("commands-join-1", commands.as_str().into())],
                ),
                text("repo-workflows-doctor-turn-on-action-f4830a1d").into(),
            ),
        }
    }
}
