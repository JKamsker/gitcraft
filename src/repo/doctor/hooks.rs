//! The hooks section's state and background work: reading the hooks folder and what the
//! commands that run hooks printed, and fixing a hook in one click.

use super::Read;
use crate::git::doctor::hook_log::{self, Run};
use crate::git::doctor::hooks::{self, Hooks};
use crate::repo::RepoTab;

/// What the hooks section asks for.
pub(crate) enum HooksAct {
    /// Make the hook of this name executable.
    MakeExecutable(String),
    /// Rewrite the hook of this name with LF line endings.
    ToLf(String),
}

/// A one-click fix of the hook file at a path.
type Fix = fn(&std::path::Path) -> Result<(), String>;

/// What the section shows.
#[derive(Default)]
pub(crate) struct HooksState {
    pub(super) found: Read<Hooks>,
    /// What the commands that run hooks printed, newest first.
    pub(super) runs: Vec<Run>,
    /// A fix is being made.
    pub(super) fixing: bool,
    /// How the last fix went.
    pub(super) fixed: Option<Result<String, String>>,
    /// An operation was running when last drawn: once it ends, what it printed is taken in.
    pub(super) was_busy: bool,
}

impl RepoTab {
    /// Takes in what an operation that just ended printed, while the section shows.
    pub(super) fn hooks_poll(&mut self) {
        let busy = self.busy_op.is_some();
        let path = self.path.clone();
        if let Some(d) = self
            .doctor
            .as_mut()
            .filter(|d| d.section == super::Section::Hooks)
        {
            if d.hooks.was_busy && !busy {
                d.hooks.runs = hook_log::runs(&path);
            }
            d.hooks.was_busy = busy;
        }
    }

    pub(super) fn hooks_act(&mut self, act: HooksAct, ctx: &egui::Context) {
        let Some(found) = self
            .doctor
            .as_ref()
            .and_then(|d| d.hooks.found.as_ref())
            .and_then(|f| f.as_ref().ok())
        else {
            return;
        };
        let folder = crate::remote::resolve(&self.path, &found.folder);
        let (name, fix, done): (String, Fix, &str) = match act {
            HooksAct::MakeExecutable(name) => (
                name,
                hooks::make_executable,
                crate::i18n::text("repo-workflows-doctor-hooks-act-63222fea"),
            ),
            HooksAct::ToLf(name) => (
                name,
                hooks::to_lf,
                crate::i18n::text("repo-workflows-doctor-hooks-act-fdf85121"),
            ),
        };
        // A script husky's stub runs is named by its path from the repository's top.
        let script = found
            .active
            .iter()
            .any(|h| h.file.as_deref() == Some(name.as_str()));
        let file = if script {
            crate::remote::join(&self.path, &name)
        } else {
            crate::remote::join(&folder, &name)
        };
        // A hooks folder of the repository's own (core.hooksPath) is often committed.
        let tracked_note = if found.hooks_path.is_some() || script {
            " If it is committed, commit the change too (with core.fileMode off, git \
             update-index --chmod=+x; eol=lf in .gitattributes keeps line endings LF)."
        } else {
            ""
        };
        if let Some(d) = &mut self.doctor {
            d.hooks.fixing = true;
            d.hooks.fixed = None;
        }
        let message = format!("{name}: {done}.{tracked_note}");
        self.then(
            ctx,
            move || fix(&file).map(|()| message),
            |tab, result, ctx| {
                if let Some(d) = &mut tab.doctor {
                    d.hooks.fixing = false;
                    d.hooks.fixed = Some(result);
                }
                if tab
                    .doctor
                    .as_ref()
                    .is_some_and(|d| d.section == super::Section::Hooks)
                {
                    tab.doctor_act(super::DoctorAct::Reload, ctx);
                }
            },
        );
    }

    /// Reads the hooks folder, and takes what was recorded of the commands that ran hooks.
    pub(super) fn load_hooks(&mut self, read: u64, ctx: &egui::Context) {
        let path = self.path.clone();
        let Some(d) = &mut self.doctor else {
            return;
        };
        d.hooks.found = None;
        d.hooks.runs = hook_log::runs(&self.path);
        if !d.hooks.fixing {
            d.hooks.fixed = None;
        }
        self.then(
            ctx,
            move || hooks::hooks(&path),
            move |tab, found, _| {
                if let Some(d) = tab.doctor.as_mut().filter(|d| d.read == read) {
                    d.hooks.found = Some(found);
                }
            },
        );
    }
}
