//! Custom commands in a repository's tab: the repository's own file (read in the background,
//! offered once trusted), the entries of the menus and the palette, and running a command.

mod run;
#[cfg(test)]
mod tests;

use std::sync::Arc;

use egui::Ui;

use super::{Action, Dialog, RepoTab};
use crate::custom_commands::{
    self as cc, CustomCommand, Offered, Placement, RepoFile, Scope, Source,
};
use crate::settings::Settings;

/// The tab's knowledge of the repository's command file.
#[derive(Default)]
pub(crate) struct TabCommands {
    /// The file as last read; `None` when there is none (or it isn't read yet).
    file: Option<RepoFile>,
    /// What was last said in a toast about an untrusted file, not to say it at every read.
    announced: Option<String>,
    /// What the menus offered when the tab was last drawn, for the palette.
    published: Arc<Published>,
}

/// Something done about custom commands, from a menu, the palette or a dialog.
pub(crate) enum CustomAct {
    /// Run `command` about `scope`: ask its prompts and confirmation first, if any.
    Pick {
        command: Box<CustomCommand>,
        scope: Scope,
    },
    /// Run it now, with these answers to its prompts.
    Run {
        command: Box<CustomCommand>,
        scope: Scope,
        answers: Vec<String>,
    },
    /// Show the repository's file, to trust it.
    Review,
    /// Trust the repository's file as it reads now (`text`).
    Trust(String),
    /// The repository's file was read: its text, or `None` when there is none.
    FileRead(Result<Option<String>, String>),
}

/// What the menus of the tab drawn offer, for menus drawn away from the tab (a file's).
#[derive(Clone, Default)]
struct Published {
    offered: Vec<Offered>,
    /// The repository has commands not (or no longer) trusted.
    review: bool,
    /// What fills a scope's empty fields: HEAD, the checked-out branch, its remote.
    defaults: Scope,
}

fn published_id() -> egui::Id {
    egui::Id::new("gitcraft-custom-commands")
}

/// The menu entries of the commands offered at `placement`, about `scope` (its empty fields
/// are the checked-out branch's, HEAD…), after a separator; nothing when there are none.
pub(crate) fn menu_items(ui: &mut Ui, placement: Placement, scope: Scope, acts: &mut Vec<Action>) {
    let Some(published) = ui
        .ctx()
        .data(|d| d.get_temp::<Arc<Published>>(published_id()))
    else {
        return;
    };
    let here: Vec<&Offered> = published
        .offered
        .iter()
        .filter(|o| o.command.placement == placement)
        .collect();
    if here.is_empty() && !published.review {
        return;
    }
    ui.separator();
    let scope = filled(scope, &published.defaults);
    for offered in here {
        let tip = match offered.source {
            Source::Global => "Custom command (Settings)",
            Source::Repository => "Custom command of this repository",
        };
        if ui
            .button(offered.command.entry_label())
            .on_hover_text(tip)
            .clicked()
        {
            acts.push(pick(&offered.command, scope.clone()));
        }
    }
    if published.review
        && ui
            .button("Review repository commands…")
            .on_hover_text(format!(
                "{} has commands not trusted yet: read them first",
                cc::REPO_FILE
            ))
            .clicked()
    {
        acts.push(Action::Custom(CustomAct::Review));
    }
}

/// `scope` with its empty fields taken from `defaults`.
fn filled(mut scope: Scope, defaults: &Scope) -> Scope {
    for (field, default) in [
        (&mut scope.sha, &defaults.sha),
        (&mut scope.branch, &defaults.branch),
        (&mut scope.remote, &defaults.remote),
    ] {
        if field.is_empty() {
            field.clone_from(default);
        }
    }
    scope
}

fn pick(command: &CustomCommand, scope: Scope) -> Action {
    Action::Custom(CustomAct::Pick {
        command: Box::new(command.clone()),
        scope,
    })
}

impl RepoTab {
    /// The repository's commands that may be offered: its file's, once trusted as it reads.
    fn trusted_repo_commands(&self, trusted: &[cc::TrustedFile]) -> &[CustomCommand] {
        match &self.custom.file {
            Some(RepoFile {
                text,
                commands: Ok(list),
            }) if cc::is_trusted(trusted, &self.trust_key(), text) => list,
            _ => &[],
        }
    }

    /// Whether the repository has a command file the user hasn't trusted as it reads now.
    fn needs_review(&self, trusted: &[cc::TrustedFile]) -> bool {
        self.custom
            .file
            .as_ref()
            .is_some_and(|f| !cc::is_trusted(trusted, &self.trust_key(), &f.text))
    }

    /// What a trust is kept under: the repository's folder, as gitcraft opens it.
    fn trust_key(&self) -> String {
        self.path.to_string_lossy().into_owned()
    }

    /// The checked-out branch's upstream remote, else the default one.
    fn branch_remote(&self, branch: &str) -> String {
        self.refs
            .local
            .iter()
            .find(|b| b.name == branch)
            .and_then(|b| self.split_upstream(&b.upstream))
            .map(|(remote, _)| remote)
            .or_else(|| self.default_remote())
            .unwrap_or_default()
    }

    /// HEAD, the checked-out branch and its remote: what a command is about by default.
    pub(super) fn default_scope(&self) -> Scope {
        let branch = self.refs.current.clone().unwrap_or_default();
        Scope {
            sha: self.refs.head.clone(),
            remote: self.branch_remote(&branch),
            branch,
            file: String::new(),
        }
    }

    /// The scope of a branch's menu: its tip, its name, its remote.
    pub(super) fn branch_scope(&self, name: &str, target: &str, remote: Option<&str>) -> Scope {
        Scope {
            sha: target.to_owned(),
            branch: name.to_owned(),
            remote: remote.map_or_else(|| self.branch_remote(name), str::to_owned),
            file: String::new(),
        }
    }

    /// Makes what this tab offers known to the menus drawn this frame (see [`menu_items`]) and
    /// to the palette.
    pub(super) fn publish_custom_commands(&mut self, ctx: &egui::Context) {
        let settings = Settings::get(ctx);
        let trusted = &settings.trusted_command_files;
        let repo = self.trusted_repo_commands(trusted);
        let offered = Placement::ALL
            .iter()
            .flat_map(|&p| cc::offered_at(p, &settings.custom_commands, repo))
            .collect();
        let published = Published {
            offered,
            review: self.needs_review(trusted),
            defaults: self.default_scope(),
        };
        let published = Arc::new(published);
        ctx.data_mut(|d| d.insert_temp(published_id(), Arc::clone(&published)));
        self.custom.published = published;
    }

    /// The palette's entries: the commands placed there, and reviewing the repository's.
    pub(super) fn custom_palette_items(&self) -> Vec<(String, String, Action)> {
        let published = &self.custom.published;
        let mut items: Vec<(String, String, Action)> = published
            .offered
            .iter()
            .filter(|o| o.command.placement == Placement::Palette)
            .map(|o| {
                let hint = match o.source {
                    Source::Global => "custom command",
                    Source::Repository => "repository command",
                };
                (
                    o.command.entry_label(),
                    hint.to_owned(),
                    pick(&o.command, published.defaults.clone()),
                )
            })
            .collect();
        if published.review {
            items.push((
                "Review repository commands…".to_owned(),
                cc::REPO_FILE.to_owned(),
                Action::Custom(CustomAct::Review),
            ));
        }
        items
    }

    /// Reads the repository's command file again, in the background.
    pub(super) fn read_custom_commands(&self, ctx: &egui::Context) {
        let repo = self.path.clone();
        self.then(
            ctx,
            move || read_file(&repo),
            |tab, read, ctx| tab.custom_act(CustomAct::FileRead(read), ctx),
        );
    }

    pub(super) fn custom_act(&mut self, act: CustomAct, ctx: &egui::Context) {
        match act {
            CustomAct::Pick { command, scope } => {
                if command.confirm || !command.prompts.is_empty() {
                    let branches = self.branch_names();
                    self.dialog = Some(Dialog::Custom(Box::new(
                        super::dialogs::CustomDialog::ask(*command, scope, &branches),
                    )));
                } else {
                    self.run_custom(&command, &scope, &[], ctx);
                }
            }
            CustomAct::Run {
                command,
                scope,
                answers,
            } => self.run_custom(&command, &scope, &answers, ctx),
            CustomAct::Review => {
                if let Some(file) = &self.custom.file {
                    self.dialog = Some(Dialog::Custom(Box::new(
                        super::dialogs::CustomDialog::review(file.clone()),
                    )));
                }
            }
            CustomAct::Trust(text) => {
                let mut settings = Settings::get(ctx);
                cc::trust(
                    &mut settings.trusted_command_files,
                    &self.trust_key(),
                    &text,
                );
                settings.set(ctx);
            }
            CustomAct::FileRead(read) => self.custom_file_read(read, ctx),
        }
    }

    /// Takes in the repository's file as read; says once, in a toast, when it needs reviewing.
    fn custom_file_read(&mut self, read: Result<Option<String>, String>, ctx: &egui::Context) {
        let file = match read {
            Ok(text) => text.map(RepoFile::new),
            // Not readable: shown as a file whose commands can't be used, to say why.
            Err(e) => Some(RepoFile {
                text: String::new(),
                commands: Err(e),
            }),
        };
        self.custom.file = file;
        let trusted = Settings::get(ctx).trusted_command_files;
        let pending = self
            .custom
            .file
            .as_ref()
            .filter(|_| self.needs_review(&trusted))
            .map(|f| f.text.clone());
        if pending.is_some() && pending != self.custom.announced {
            self.finished.push((
                format!(
                    "{} has its own custom commands: review them (palette or a menu) to use \
                     them",
                    self.name
                ),
                false,
            ));
        }
        self.custom.announced = pending;
    }

    /// Every branch a prompt can offer: local ones, then remote ones.
    fn branch_names(&self) -> Vec<String> {
        let local = self.refs.local.iter().map(|b| b.name.clone());
        let remote = self
            .refs
            .remotes
            .iter()
            .flat_map(|(_, list)| list.iter().map(|b| b.name.clone()));
        local.chain(remote).collect()
    }
}

/// The command file of the repository at `repo`: its text, `None` when there is none. Never
/// through a symbolic link, nor more than [`cc::REPO_FILE_MAX`] bytes. On another machine, a
/// file that is a link or too large reads as none (it is checked there, in one round trip).
fn read_file(repo: &std::path::Path) -> Result<Option<String>, String> {
    let max = cc::REPO_FILE_MAX as u64;
    let cant = |e: &dyn std::fmt::Display| format!("{} can't be read: {e}", cc::REPO_FILE);
    let bytes = match crate::remote::read_within(repo, cc::REPO_FILE, max) {
        Some(read) => read.map_err(|e| cant(&e))?,
        None => read_here(&repo.join(cc::REPO_FILE), max).map_err(|e| cant(&e))?,
    };
    bytes
        .map(|b| String::from_utf8(b).map_err(|_| format!("{} isn't UTF-8 text", cc::REPO_FILE)))
        .transpose()
}

/// [`read_file`] for a repository of this machine.
fn read_here(path: &std::path::Path, max: u64) -> std::io::Result<Option<Vec<u8>>> {
    use std::io::{Error, ErrorKind};
    let meta = match std::fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    if meta.file_type().is_symlink() {
        return Err(Error::other("it is a symbolic link"));
    }
    if !meta.is_file() {
        return Err(Error::other("it isn't a file"));
    }
    if meta.len() > max {
        return Err(Error::other(format!(
            "it is larger than {} KiB",
            max / 1024
        )));
    }
    std::fs::read(path).map(Some)
}
