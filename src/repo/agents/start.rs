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
    focus: bool,
    repo: std::path::PathBuf,
    /// Where the folder suggested for a branch goes: beside the main worktree.
    folder_for: Box<dyn Fn(&str) -> String>,
}

impl StartForm {
    pub(super) fn new(tab: &RepoTab) -> Self {
        let main = tab.main_worktree().unwrap_or(&tab.path).to_path_buf();
        let taken: Vec<std::path::PathBuf> = tab.worktrees.iter().map(|w| w.path.clone()).collect();
        let folder_for = Box::new(move |branch: &str| suggest_folder(&main, branch, &taken));
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
            focus: true,
            repo: tab.path.clone(),
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
        if let Err(why) = &folder {
            ui.colored_label(t.danger, why);
        }
        ui.add_space(6.0);
        self.agent_choice(ui, &t);
        ui.add_space(6.0);
        ui.label(
            RichText::new(
                "The worktree setup recipe runs first, in the same terminal (Worktree setup… \
                 in the palette).",
            )
            .small()
            .color(t.text_faint),
        );
        ui.add_space(10.0);
        let usable = |s: &str| !s.trim().is_empty() && !s.trim().starts_with('-');
        let agent = self.agent_command();
        let valid = usable(&self.branch)
            && !self.branch.contains(char::is_whitespace)
            && usable(&self.base)
            && folder.is_ok()
            && usable(&agent);
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
                })));
            }
            if ui
                .add(egui::Button::new("Cancel").min_size(egui::vec2(80.0, 30.0)))
                .clicked()
            {
                act = Some(AgentAct::CloseDialog);
            }
        });
        act
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

/// The folder beside `main` for `branch` (`app` + `agent/x` → `app-agent-x`), as git of its
/// machine takes it, not one of the worktrees `taken` (`-2`, `-3`… then).
fn suggest_folder(main: &std::path::Path, branch: &str, taken: &[std::path::PathBuf]) -> String {
    let suffix = branch.trim().replace(['/', '\\', ' '], "-");
    let (parent, name, sep) = match crate::remote::parse(main) {
        Some(target) => {
            let parent = crate::remote::parent_there(&target.path).trim_end_matches('/');
            let name = target.path.rsplit('/').next().unwrap_or("repo").to_owned();
            (parent.to_owned(), name, "/")
        }
        None => (
            main.parent().unwrap_or(main).display().to_string(),
            main.file_name()
                .map_or_else(|| "repo".to_owned(), |n| n.to_string_lossy().into_owned()),
            std::path::MAIN_SEPARATOR_STR,
        ),
    };
    let base = format!(
        "{}{sep}{name}-{suffix}",
        parent.trim_end_matches(['/', '\\'])
    );
    let is_taken = |candidate: &str| {
        taken.iter().any(|w| {
            crate::remote::git_path(w) == candidate || w.display().to_string() == candidate
        }) || (!crate::remote::is_remote(main) && std::path::Path::new(candidate).exists())
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
        let enter = ctx.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.shift);
        let in_linked = self.worktrees.iter().any(|w| w.current && !w.main);
        let response = egui::Modal::new(egui::Id::new("gitcraft-agent-dialog"))
            .frame(frame)
            .backdrop_color(egui::Color32::from_black_alpha(110))
            .show(ctx, |ui| {
                ui.set_width(520.0);
                match dialog {
                    super::AgentDialog::Start(form) => {
                        form.ui(ui, enter && !ui.ctx().egui_wants_keyboard_input())
                    }
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

    #[test]
    fn a_task_folder_goes_beside_the_main_worktree_and_is_never_taken() {
        let main = Path::new("ssh://build/home/me/app");
        assert_eq!(
            suggest_folder(main, "agent/fix ü", &[]),
            "/home/me/app-agent-fix-ü"
        );
        let taken = [PathBuf::from("ssh://build/home/me/app-agent-x")];
        assert_eq!(
            suggest_folder(main, "agent/x", &taken),
            "/home/me/app-agent-x-2"
        );
    }
}
