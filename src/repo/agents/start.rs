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
        ui.label(
            RichText::new(crate::i18n::text("repo-workflows-agents-ui-97c3e67d"))
                .font(theme::semibold(15.0)),
        );
        ui.label(
            RichText::new(crate::i18n::text("repo-workflows-agents-ui-3553b76b")).color(t.text_dim),
        );
        ui.add_space(10.0);
        caption(
            ui,
            crate::i18n::text("repo-workflows-agents-ui-4bc74b21"),
            &t,
        );
        let edit = ui.add(
            egui::TextEdit::multiline(&mut self.task)
                .hint_text(crate::i18n::text("repo-workflows-agents-ui-27cec493"))
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
        caption(
            ui,
            crate::i18n::text("repo-workflows-agents-ui-52656e81"),
            &t,
        );
        if single(ui, &mut self.branch).changed() {
            // As in the other dialogs: a branch name has no spaces.
            self.branch = self.branch.replace(' ', "-");
            self.branch_edited = true;
            if !self.folder_edited {
                self.folder = (self.folder_for)(&self.branch);
            }
        }
        caption(
            ui,
            crate::i18n::text("repo-workflows-agents-ui-e8a96feb"),
            &t,
        );
        single(ui, &mut self.base);
        caption(
            ui,
            crate::i18n::text("repo-workflows-agents-ui-74ccd433"),
            &t,
        );
        if single(ui, &mut self.folder).changed() {
            self.folder_edited = true;
        }
        let folder = crate::remote::folder_for_git(&self.repo, &self.folder);
        ui.add_space(6.0);
        self.agent_choice(ui, &t);
        ui.add_space(6.0);
        ui.label(
            RichText::new(crate::i18n::text("repo-workflows-agents-ui-74296b58"))
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
            if crate::repo::primary_button(
                ui,
                crate::i18n::text("repo-workflows-agents-ui-e4bb9f1e"),
                valid,
                90.0,
            )
            .clicked()
                || (enter && valid)
            {
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
                .add(
                    egui::Button::new(crate::i18n::text("repo-workflows-agents-ui-19766ed6"))
                        .min_size(egui::vec2(80.0, 30.0)),
                )
                .clicked()
            {
                act = Some(AgentAct::CloseDialog);
            }
            ui.label(
                RichText::new(crate::i18n::message(
                    "repo-workflows-agents-ui-44e1cc2b",
                    &[(
                        "crate-manual-command-1",
                        (crate::manual::command()).clone().into(),
                    )],
                ))
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
            return Some(crate::i18n::text("repo-workflows-agents-problem-5fbb1fb2").to_owned());
        }
        if branch.starts_with('-') {
            return Some(crate::i18n::text("repo-workflows-agents-problem-8bffbdf3").to_owned());
        }
        if self.branches.iter().any(|b| b == branch) {
            return Some(crate::i18n::message(
                "repo-workflows-agents-problem-80f33652",
                &[("branch", (branch).clone().into())],
            ));
        }
        let base = self.base.trim();
        if base.is_empty() || base.starts_with('-') {
            return Some(crate::i18n::text("repo-workflows-agents-problem-c3177bf6").to_owned());
        }
        if let Err(why) = folder {
            return Some(why.clone());
        }
        let typed = self.folder.trim();
        if typed.is_empty() {
            return Some(crate::i18n::text("repo-workflows-agents-problem-bbc12752").to_owned());
        }
        if crate::remote::is_remote(&self.repo) && typed.starts_with('~') {
            return Some(crate::i18n::text("repo-workflows-agents-problem-6eabbce8").to_owned());
        }
        if agent.is_empty() {
            return Some(crate::i18n::text("repo-workflows-agents-problem-97c58cf1").to_owned());
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
        caption(
            ui,
            crate::i18n::text("repo-workflows-agents-agent-choice-11b39c93"),
            t,
        );
        let selected = KNOWN.get(self.agent).map_or(
            crate::i18n::text("repo-workflows-agents-agent-choice-57b9f7ac"),
            |cli| cli.label,
        );
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
                ui.selectable_value(
                    &mut self.agent,
                    KNOWN.len(),
                    crate::i18n::text("repo-workflows-agents-agent-choice-57b9f7ac"),
                );
            });
        match KNOWN.get(self.agent) {
            None => {
                single(ui, &mut self.custom);
                ui.label(
                    RichText::new(crate::i18n::text(
                        "repo-workflows-agents-agent-choice-92a24920",
                    ))
                    .small()
                    .color(t.text_faint),
                );
            }
            Some(cli) if cli.prompt_flag.is_some() => {
                ui.checkbox(
                    &mut self.give_prompt,
                    crate::i18n::text("repo-workflows-agents-agent-choice-c8daf5c0"),
                );
            }
            Some(cli) => {
                ui.label(
                    RichText::new(crate::i18n::message(
                        "repo-workflows-agents-agent-choice-2b8f8124",
                        &[("cli-label-1", (cli.label).clone().into())],
                    ))
                    .small()
                    .color(t.text_faint),
                );
            }
        }
        if self.takes_hooks() {
            ui.checkbox(
                &mut self.hooks,
                crate::i18n::text("repo-workflows-agents-agent-choice-e383f131"),
            )
            .on_hover_text(crate::i18n::text(
                "repo-workflows-agents-agent-choice-36a10f87",
            ));
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

#[cfg(test)]
mod fluent_partition_tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};

    // Supplied state only: no RepoTab/App construction, discovery, terminal or Git run.
    fn supplied_form() -> StartForm {
        StartForm {
            task: "Keep --literal-pathspecs and refs/heads/topic".into(),
            branch: "topic/Überprüfung".into(),
            branch_edited: true,
            folder: "/supplied/work tree".into(),
            folder_edited: true,
            base: "refs/heads/main".into(),
            agent: KNOWN.len(),
            custom: "codex --model exact-model".into(),
            give_prompt: true,
            hooks: false,
            focus: false,
            repo: "/supplied/repository".into(),
            branches: vec![],
            folder_for: Box::new(|_| "unexpected-folder-resolution".into()),
        }
    }

    #[test]
    fn fluent_partition_actual_start_form_keeps_action_payload_in_both_languages()
    -> Result<(), String> {
        for (language, title, task, start) in [
            ("en-US", "Start agent task", "Task", "Start"),
            ("de", "Agent-Aufgabe starten", "Aufgabe", "Starten"),
        ] {
            crate::i18n::set_language(language);
            let mut initialized = false;
            let h = Harness::new_ui_state(
                |ui, form: &mut StartForm| {
                    if !initialized {
                        crate::theme::install_fonts(ui.ctx());
                        initialized = true;
                        ui.ctx().request_discard("Install supplied test fonts");
                        return;
                    }
                    let _ = form.ui(ui, false);
                },
                supplied_form(),
            );
            h.get_by_label(title);
            h.get_by_label(task);
            h.get_by_label(start);
            assert_eq!(h.state().agent_command(), "codex --model exact-model");
            assert_eq!(h.state().branch, "topic/Überprüfung");
            let mut recording_initialized = false;
            let recorded = Harness::new_ui_state(
                |ui, state: &mut (StartForm, Option<AgentAct>)| {
                    if !recording_initialized {
                        crate::theme::install_fonts(ui.ctx());
                        recording_initialized = true;
                        ui.ctx().request_discard("Install supplied test fonts");
                        return;
                    }
                    state.1 = state.0.ui(ui, true);
                },
                (supplied_form(), None),
            );
            let Some(AgentAct::Start(payload)) = &recorded.state().1 else {
                return Err("expected recorded Start action".into());
            };
            assert_eq!(payload.agent, "codex --model exact-model");
            assert_eq!(payload.branch, "topic/Überprüfung");
            assert_eq!(payload.folder, "/supplied/work tree");
            assert_eq!(payload.base, "refs/heads/main");
            assert_eq!(
                payload.prompt,
                "Keep --literal-pathspecs and refs/heads/topic"
            );
        }
        crate::i18n::set_language("en-US");
        Ok(())
    }

    #[test]
    fn fluent_partition_actual_validation_preserves_named_repository_content() {
        let mut form = supplied_form();
        form.branches.push(form.branch.clone());
        for (language, expected) in [
            (
                "en-US",
                "A branch named topic/Überprüfung exists already: pick another name",
            ),
            (
                "de",
                "Ein Branch namens topic/Überprüfung existiert bereits. Wählen Sie einen anderen Namen.",
            ),
        ] {
            crate::i18n::set_language(language);
            assert_eq!(
                form.problem(&Ok(form.folder.clone()), &form.agent_command())
                    .as_deref(),
                Some(expected)
            );
        }
        crate::i18n::set_language("en-US");
    }
}
