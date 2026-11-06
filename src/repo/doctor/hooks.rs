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
}

impl RepoTab {
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
            HooksAct::MakeExecutable(name) => (name, hooks::make_executable, "made executable"),
            HooksAct::ToLf(name) => (name, hooks::to_lf, "saved with LF line endings"),
        };
        let file = crate::remote::join(&folder, &name);
        if let Some(d) = &mut self.doctor {
            d.hooks.fixing = true;
            d.hooks.fixed = None;
        }
        let message = format!("{name}: {done}");
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
