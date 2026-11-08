//! The _Start agent task…_ dialog: what to do, the branch and folder for it, and which agent.

use egui::{RichText, Ui};

use super::AgentAct;
use crate::agent::tasks::{self, KNOWN};
use crate::repo::{Action, RepoTab};
use crate::theme::{self, Tokens};

/// What the dialog starts.
pub(crate) struct NewTask {
    pub branch: String,
    /// As git of the repository's machine takes it.
    pub folder: String,
    pub base: String,
    /// The agent's command line, without the prompt.
    pub agent: String,
    pub prompt: String,
    /// Install the hooks that save each turn as a checkpoint in the worktree.
    pub hooks: bool,
}

pub(crate) struct StartForm {
    task: String,
    branch: String,
    branch_edited: bool,
    folder: String,
    folder_edited: bool,
    base: String,
    /// Which of [`KNOWN`]; one past them for a command line of one's own.
    agent: usize,
    custom: String,
    give_prompt: bool,
    /// Save each turn as a checkpoint (Claude Code and Codex).
    hooks: bool,
    focus: bool,
    repo: std::path::PathBuf,
    /// The local branches, which the new one mustn't be.
    branches: Vec<String>,
    /// Where the folder suggested for a branch goes: under the saved repository root.
    folder_for: Box<dyn Fn(&str) -> String + Send>,
}

impl StartForm {
    pub(super) fn new(tab: &RepoTab) -> Self {
        let main = tab.worktrees.first().map_or(&tab.path, |w| &w.path).clone();
        let taken: Vec<std::path::PathBuf> = tab.worktrees.iter().map(|w| w.path.clone()).collect();
        let root = crate::git::resolve_worktree_root(&main, &tab.worktree_root)
            .unwrap_or_else(|_| crate::git::default_worktree_root(&main));
        let folder_for = Box::new(move |branch: &str| suggest_folder(&root, branch, &taken));
        let base = tab
            .refs
            .current
            .clone()
            .unwrap_or_else(|| "HEAD".to_owned());
        let mut form = Self {
            task: String::new(),
            branch: tasks::branch_for(""),
            branch_edited: false,
            folder: String::new(),
            folder_edited: false,
            base,
            agent: 0,
            custom: String::new(),
            give_prompt: true,
            hooks: true,
            focus: true,
            repo: tab.path.clone(),
            branches: tab.refs.local.iter().map(|b| b.name.clone()).collect(),
            folder_for,
        };
        form.folder = (form.folder_for)(&form.branch);
        form
    }

    fn agent_command(&self) -> String {
        match KNOWN.get(self.agent) {
            Some(cli) => cli.program.to_owned(),
            None => self.custom.trim().to_owned(),
        }
    }

    /// Draws the dialog's body; what it asks for, if anything.
    pub(super) fn ui(&mut self, ui: &mut Ui, enter: bool) -> Option<AgentAct> {
        let t = Tokens::get(ui.ctx());
        ui.label(RichText::new("Start agent task").font(theme::semibold(15.0)));
        ui.label(
            RichText::new(
                "A branch and a worktree of its own, then a terminal there running the agent.",
            )
            .color(t.text_dim),
        );
        ui.add_space(10.0);
        caption(ui, "Task", &t);
        let edit = ui.add(
            egui::TextEdit::multiline(&mut self.task)
                .hint_text("What the agent should do (optional)")
                .desired_rows(3)
                .desired_width(f32::INFINITY),
        );
        if std::mem::take(&mut self.focus) {
            edit.request_focus();
        }
        if edit.changed() && !self.branch_edited {
            self.branch = tasks::branch_for(&self.task);
            if !self.folder_edited {
                self.folder = (self.folder_for)(&self.branch);
            }
        }
        ui.add_space(6.0);
        caption(ui, "Branch", &t);
        if single(ui, &mut self.branch).changed() {
            // As in the other dialogs: a branch name has no spaces.
            self.branch = self.branch.replace(' ', "-");
            self.branch_edited = true;
            if !self.folder_edited {
                self.folder = (self.folder_for)(&self.branch);
            }
        }
        caption(ui, "Starting at", &t);
        single(ui, &mut self.base);
        caption(ui, "Folder", &t);
        if single(ui, &mut self.folder).changed() {
            self.folder_edited = true;
        }
        let folder = crate::remote::folder_for_git(&self.repo, &self.folder);
        ui.add_space(6.0);
        self.agent_choice(ui, &t);
        ui.add_space(6.0);
        ui.label(
            RichText::new(
                "If the repository has a worktree setup recipe (Worktree setup… in the \
                 palette), it runs first, in the same terminal.",
            )
            .small()
            .color(t.text_faint),
        );
        ui.add_space(10.0);
        let agent = self.agent_command();
        let problem = self.problem(&folder, &agent);
        if let Some(problem) = &problem {
            ui.colored_label(t.danger, problem);
            ui.add_space(4.0);
        }
        let valid = problem.is_none();
        let mut act = None;
        ui.horizontal(|ui| {
            if crate::repo::primary_button(ui, "Start", valid, 90.0).clicked() || (enter && valid) {
                act = Some(AgentAct::Start(Box::new(NewTask {
                    branch: self.branch.trim().to_owned(),
                    folder: folder.clone().unwrap_or_default(),
                    base: self.base.trim().to_owned(),
                    agent,
                    prompt: if self.give_prompt {
                        self.task.trim().to_owned()
                    } else {
                        String::new()
                    },
                    hooks: self.hooks && self.takes_hooks(),
                })));
            }
            if ui
                .add(egui::Button::new("Cancel").min_size(egui::vec2(80.0, 30.0)))
                .clicked()
            {
                act = Some(AgentAct::CloseDialog);
            }
            ui.label(
                RichText::new(format!("{}+Enter to start", crate::manual::command()))
                    .small()
                    .color(t.text_faint),
            );
        });
        act
    }

    /// What keeps the task from starting, in words for the user.
    fn problem(&self, folder: &Result<String, String>, agent: &str) -> Option<String> {
        let branch = self.branch.trim();
        if branch.is_empty() {
            return Some("Name the branch the agent works on".to_owned());
        }
        if branch.starts_with('-') {
            return Some("A branch name can't start with -".to_owned());
        }
        if self.branches.iter().any(|b| b == branch) {
            return Some(format!(
                "A branch named {branch} exists already: pick another name"
            ));
        }
        let base = self.base.trim();
        if base.is_empty() || base.starts_with('-') {
            return Some("Say where the branch starts: a branch, tag or commit".to_owned());
        }
        if let Err(why) = folder {
            return Some(why.clone());
        }
        let typed = self.folder.trim();
        if typed.is_empty() {
            return Some("Pick the folder for the worktree".to_owned());
        }
        if crate::remote::is_remote(&self.repo) && typed.starts_with('~') {
            return Some(
                "Give the folder's full path on that machine (/home/…): git doesn't read ~"
                    .to_owned(),
            );
        }
        if agent.is_empty() {
            return Some("Type the command that starts the agent".to_owned());
        }
        None
    }

    /// Whether the agent picked has hooks gitcraft installs: Claude Code and Codex.
    fn takes_hooks(&self) -> bool {
        KNOWN
            .get(self.agent)
            .is_some_and(|cli| matches!(cli.program, "claude" | "codex"))
    }

    fn agent_choice(&mut self, ui: &mut Ui, t: &Tokens) {
        caption(ui, "Agent", t);
        let selected = KNOWN
            .get(self.agent)
            .map_or("Command of my own", |cli| cli.label);
        egui::ComboBox::from_id_salt("agent-cli")
            .width(ui.available_width())
            .selected_text(selected)
            .show_ui(ui, |ui| {
                for (i, cli) in KNOWN.iter().enumerate() {
                    ui.selectable_value(
                        &mut self.agent,
                        i,
                        format!("{}  ({})", cli.label, cli.program),
                    );
                }
                ui.selectable_value(&mut self.agent, KNOWN.len(), "Command of my own");
            });
        match KNOWN.get(self.agent) {
            None => {
                single(ui, &mut self.custom);
                ui.label(
                    RichText::new("Run by sh in the worktree, e.g. claude --model opus")
                        .small()
                        .color(t.text_faint),
                );
            }
            Some(cli) if cli.prompt_flag.is_some() => {
                ui.checkbox(
                    &mut self.give_prompt,
                    "Give it the task as its first message",
                );
            }
            Some(cli) => {
                ui.label(
                    RichText::new(format!(
                        "{} takes no first message: type it there",
                        cli.label
                    ))
                    .small()
                    .color(t.text_faint),
                );
            }
        }
        if self.takes_hooks() {
            ui.checkbox(&mut self.hooks, "Save each turn to review on its own")
                .on_hover_text(
                    "Installs the agent's hooks in the worktree: after each turn they run gitcraft \
                     checkpoint, and Uncommitted changes lists the turns, each to diff on its own",
                );
        }
    }
}

fn caption(ui: &mut Ui, text: &str, t: &Tokens) {
    ui.label(RichText::new(text).color(t.text_dim).small());
}

fn single(ui: &mut Ui, text: &mut String) -> egui::Response {
    ui.add(
        egui::TextEdit::singleline(text)
            .desired_width(f32::INFINITY)
            .margin(egui::Margin::symmetric(8, 6)),
    )
}

/// A branch folder under `root`, avoiding the worktrees `taken` (`-2`, `-3`… then).
fn suggest_folder(root: &std::path::Path, branch: &str, taken: &[std::path::PathBuf]) -> String {
    let base = crate::remote::git_path(&crate::git::branch_worktree_folder(root, branch));
    let is_taken = |candidate: &str| {
        taken.iter().any(|w| {
            crate::remote::git_path(w) == candidate || w.display().to_string() == candidate
        }) || (!crate::remote::is_remote(root) && std::path::Path::new(candidate).exists())
    };
    if !is_taken(&base) {
        return base;
    }
    (2..1000)
        .map(|n| format!("{base}-{n}"))
        .find(|c| !is_taken(c))
        .unwrap_or(base)
}

impl RepoTab {
    /// The agent dialogs, over the tab.
    pub(in crate::repo) fn agent_dialogs(&mut self, ctx: &egui::Context, acts: &mut Vec<Action>) {
        let Some(dialog) = &mut self.agents.dialog else {
            return;
        };
        let t = Tokens::get(ctx);
        let frame = egui::Frame::new()
            .fill(t.card)
            .stroke(egui::Stroke::new(1.0, t.field_border))
            .corner_radius(t.radius_lg)
            .inner_margin(egui::Margin::same(18))
            .shadow(ctx.global_style().visuals.window_shadow);
        // Ctrl+Enter: Enter alone is a new line in the task, and is typed in the fields.
        let enter = ctx.input(|i| i.key_pressed(egui::Key::Enter) && i.modifiers.command);
        let in_linked = self.worktrees.iter().any(|w| w.current && !w.main);
        let response = egui::Modal::new(egui::Id::new("gitcraft-agent-dialog"))
            .frame(frame)
            .backdrop_color(egui::Color32::from_black_alpha(110))
            .show(ctx, |ui| {
                ui.set_width(520.0);
                match dialog {
                    super::AgentDialog::Start(form) => form.ui(ui, enter),
                    super::AgentDialog::Recipe(form) => form.ui(ui, in_linked),
                }
            });
        if response.should_close() {
            acts.push(Action::Agent(AgentAct::CloseDialog));
        }
        if let Some(act) = response.inner {
            acts.push(Action::Agent(act));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::suggest_folder;
    use std::path::{Path, PathBuf};

    fn form(branch: &str, folder: &str, repo: &str) -> super::StartForm {
        super::StartForm {
            task: String::new(),
            branch: branch.to_owned(),
            branch_edited: false,
            folder: folder.to_owned(),
            folder_edited: false,
            base: "main".to_owned(),
            agent: 0,
            custom: String::new(),
            give_prompt: true,
            hooks: false,
            focus: false,
            repo: PathBuf::from(repo),
            branches: vec!["main".to_owned(), "agent/x".to_owned()],
            folder_for: Box::new(|_| String::new()),
        }
    }

    #[test]
    fn the_dialog_says_what_keeps_it_from_starting() {
        let ok = form("agent/new", "/w/app-new", "/w/app");
        assert_eq!(ok.problem(&Ok("/w/app-new".to_owned()), "claude"), None);
        let taken = form("agent/x", "/w/app-x", "/w/app");
        let said = taken
            .problem(&Ok(String::new()), "claude")
            .unwrap_or_default();
        assert!(said.contains("exists already"), "{said}");
        assert!(
            form("", "/w", "/w/app")
                .problem(&Ok(String::new()), "claude")
                .is_some()
        );
        let no_agent = form("agent/new", "/w/app-new", "/w/app");
        assert!(
            no_agent
                .problem(&Ok(String::new()), "")
                .is_some_and(|p| p.contains("command"))
        );
        let home = form("agent/new", "~/app-new", "ssh://h/home/me/app");
        assert!(
            home.problem(&Ok("~/app-new".to_owned()), "claude")
                .is_some_and(|p| p.contains("full path"))
        );
    }

    #[test]
    fn a_task_folder_uses_the_root_and_is_never_taken() {
        let main = Path::new("ssh://build/home/me/app.wt");
        assert_eq!(
            suggest_folder(main, "agent/fix ü", &[]),
            "/home/me/app.wt/agent/fix-ü"
        );
        let taken = [PathBuf::from("ssh://build/home/me/app.wt/agent/x")];
        assert_eq!(
            suggest_folder(main, "agent/x", &taken),
            "/home/me/app.wt/agent/x-2"
        );
    }

    /// Agent tasks use the configured root when opened from a linked worktree.
    #[test]
    fn a_task_form_uses_the_saved_root_of_the_main_checkout() {
        let ctx = egui::Context::default();
        let mut tab =
            crate::repo::RepoTab::new_deferred("ssh://build/elsewhere/linked".into(), &ctx);
        tab.worktrees = vec![crate::git::Worktree {
            path: "ssh://build/home/me/app".into(),
            main: true,
            ..Default::default()
        }];
        tab.worktree_root = "custom.worktrees".to_owned();
        let form = super::StartForm::new(&tab);
        assert_eq!(
            (form.folder_for)("feat/awesome-stuff"),
            "/home/me/app/custom.worktrees/feat/awesome-stuff"
        );
    }
}
