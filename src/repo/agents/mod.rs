//! Coding agents in the tab: _Start agent task…_ (a branch, a worktree and a terminal running
//! the agent there), the worktree setup recipe every new worktree gets, and the AGENTS section
//! listing the tasks with their uncommitted changes. The logic is in [`crate::agent`].

mod handoff;
mod panel;
mod recipe_editor;
mod start;
mod turns;

use std::path::PathBuf;

use crate::agent::{AgentTask, Launch, PendingLaunches, Recipe, tasks};
use crate::repo::{OpKind, OpenRequest, Refresh, RepoTab};
use crate::terminal::Terminal;

pub(in crate::repo) use handoff::{TrayAct, note_panel};
pub(super) use recipe_editor::RecipeForm;
pub(super) use start::StartForm;

/// What the tab knows of its repository's agents.
#[derive(Default)]
pub(crate) struct AgentsState {
    pub(super) tasks: Vec<AgentTask>,
    /// The worktrees the tasks were last read for (their folders), to read them again when
    /// those change.
    read_for: Option<Vec<PathBuf>>,
    reading: bool,
    pub(super) error: Option<String>,
    pub(super) dialog: Option<AgentDialog>,
    /// A worktree being made by the operation so labelled: what to run in it once it is there.
    made: Option<Made>,
    /// The agent turns saved in this worktree (see [`crate::agent::checkpoint`]).
    pub(super) turns: Vec<crate::agent::checkpoint::Turn>,
    /// The history read the turns were read for: saving a turn moves a ref, which reads the
    /// history again.
    turns_for: Option<u64>,
    /// This worktree's top folder as git names it, which its turns are marked with.
    turns_top: Option<String>,
    turns_reading: bool,
    /// Notes for the agent, and the one being written.
    pub(in crate::repo) tray: handoff::Tray,
}

/// A worktree an operation is making.
struct Made {
    label: String,
    worktree: PathBuf,
    /// The agent to start there, and its task to list, for an agent task.
    agent: Option<(String, AgentTask)>,
    /// Install the hooks that save each turn as a checkpoint there first.
    hooks: bool,
}

pub(super) enum AgentDialog {
    Start(Box<StartForm>),
    Recipe(Box<RecipeForm>),
}

/// What the agent views ask for.
pub(crate) enum AgentAct {
    OpenStart,
    Start(Box<start::NewTask>),
    OpenRecipe,
    SaveRecipe(Recipe),
    /// Runs the recipe in a terminal for the worktree this tab shows.
    RunRecipeHere(Recipe),
    CloseDialog,
    /// Starts the task's agent again, in its worktree's tab.
    Restart(AgentTask),
    /// Takes the task off the list; its worktree stays.
    Forget(String),
    /// Installs the hooks that save each agent turn, in the worktree this tab shows.
    InstallHooks,
    /// Deletes the saved turns of a session.
    ForgetTurns(String),
    /// Starts a note for the agent on a line of the diff shown.
    OpenNote(crate::repo::pr_review::LineTarget),
    Tray(handoff::TrayAct),
    /// Types a message (a conflict, a CI log, a thread: what it is) into the agent's terminal.
    Send(String, &'static str),
}

impl RepoTab {
    pub(crate) fn agent_act(&mut self, act: AgentAct, ctx: &egui::Context) {
        match act {
            AgentAct::OpenStart => {
                let form = StartForm::new(self);
                self.agents.dialog = Some(AgentDialog::Start(Box::new(form)));
            }
            AgentAct::Start(task) => self.start_agent_task(*task, ctx),
            AgentAct::OpenRecipe => self.open_recipe(ctx),
            AgentAct::SaveRecipe(recipe) => self.save_recipe(recipe, ctx),
            AgentAct::RunRecipeHere(recipe) => {
                self.agents.dialog = None;
                self.start_launch(
                    Launch {
                        label: "setup".to_owned(),
                        worktree: self.path.clone(),
                        recipe,
                        agent: None,
                        run: None,
                    },
                    ctx,
                );
            }
            AgentAct::CloseDialog => self.agents.dialog = None,
            AgentAct::Restart(task) => {
                let launch = Launch {
                    label: tasks::agent_label(&task.agent),
                    worktree: PathBuf::from(&task.path),
                    recipe: Recipe::default(),
                    agent: Some(task.agent),
                    run: None,
                };
                if crate::remote::same_dir(&launch.worktree, &self.path) {
                    self.start_launch(launch, ctx);
                } else {
                    let path = launch.worktree.clone();
                    PendingLaunches::queue(launch);
                    self.open_request = Some(OpenRequest::Worktree(path));
                }
            }
            AgentAct::InstallHooks => self.install_hooks(ctx),
            AgentAct::OpenNote(target) => self.tray_act(handoff::TrayAct::Open(target), ctx),
            AgentAct::Tray(act) => self.tray_act(act, ctx),
            AgentAct::Send(message, what) => {
                self.send_to_agent(&message, what);
            }
            AgentAct::ForgetTurns(session) => self.forget_turns(session, ctx),
            AgentAct::Forget(path) => {
                let repo = self.path.clone();
                self.then(
                    ctx,
                    move || tasks::remove(&repo, &path),
                    |tab, result, _| tab.tasks_written(result),
                );
            }
        }
    }

    /// Every frame: a launch waiting for this worktree starts its terminal, and the tasks are
    /// read again when the worktrees changed.
    pub(super) fn agents_frame(&mut self, ctx: &egui::Context) {
        crate::agent::launch::warm_up();
        self.reload_turns(ctx);
        self.load_tray(ctx);
        if let Some(launch) = PendingLaunches::take(&self.path) {
            self.start_launch(launch, ctx);
        }
        let same = self
            .agents
            .read_for
            .as_ref()
            .is_some_and(|read| read.iter().eq(self.worktrees.iter().map(|w| &w.path)));
        if self.agents.reading || self.history_gen == 0 || same {
            return;
        }
        self.agents.reading = true;
        self.agents.read_for = Some(self.worktrees.iter().map(|w| w.path.clone()).collect());
        let repo = self.path.clone();
        self.then(
            ctx,
            move || tasks::load(&repo),
            |tab, result, _| {
                tab.agents.reading = false;
                tab.tasks_written(result);
            },
        );
    }

    fn tasks_written(&mut self, result: Result<Vec<AgentTask>, String>) {
        match result {
            Ok(list) => {
                self.agents.tasks = list;
                self.agents.error = None;
            }
            Err(e) => self.agents.error = Some(e),
        }
    }

    /// Opens a terminal tab running `launch`, in its worktree.
    pub(super) fn start_launch(&mut self, launch: Launch, ctx: &egui::Context) {
        let shell = crate::terminal::default_shell(ctx);
        let spawned = launch
            .program(&shell, crate::agent::launch::windows_sh)
            .and_then(|(spec, cwd)| Terminal::spawn(&cwd, &spec, ctx));
        self.terminal_open = true;
        match spawned {
            Ok(term) => {
                self.terminals.push(term);
                self.active_terminal = self.terminals.len() - 1;
                self.terminal_error = None;
            }
            Err(e) => {
                self.terminal_error = Some(format!("Could not start {}: {e}", launch.label));
            }
        }
    }

    /// Before an operation runs: when it makes a worktree, notes it, to set it up after.
    pub(super) fn note_worktree_add(&mut self, label: &str, ops: &[Vec<String>]) {
        let Some(folder) = ops
            .iter()
            .find_map(|args| crate::agent::added_worktree(args))
        else {
            // A task's own worktree is noted when it is started.
            self.agents.made.take_if(|m| m.label != label);
            return;
        };
        if self.agents.made.as_ref().is_some_and(|m| m.label == label) {
            return;
        }
        self.agents.made = Some(Made {
            label: label.to_owned(),
            worktree: worktree_path(&self.path, folder),
            agent: None,
            hooks: false,
        });
    }

    /// After an operation: the worktree it made gets its recipe (and its agent), in its own
    /// tab when it is being opened, else in a terminal of this one.
    pub(super) fn after_worktree_add(&mut self, label: &str, ok: bool, ctx: &egui::Context) {
        let Some(made) = self.agents.made.take_if(|m| m.label == label) else {
            return;
        };
        if !ok {
            return;
        }
        let opened =
            matches!(&self.open_request, Some(OpenRequest::Worktree(p)) if *p == made.worktree);
        let repo = self.path.clone();
        let task = made.agent.as_ref().map(|(_, task)| task.clone());
        let hooks_in = made.hooks.then(|| made.worktree.clone());
        self.then(
            ctx,
            move || {
                let hooks = hooks_in.map(|wt| crate::agent::hooks::install(&wt));
                let recipe = Recipe::load(&repo);
                let listed = task.map(|mut task| {
                    // As git lists it, which the worktrees shown are compared with.
                    if let Ok(top) = crate::git::toplevel(std::path::Path::new(&task.path)) {
                        task.path = top.display().to_string();
                    }
                    // The commit it starts from, for comparing attempts later.
                    if let Ok(hash) =
                        crate::git::run(Some(&repo), &["rev-parse", "--verify", "-q", &task.base])
                    {
                        task.base = hash.trim().to_owned();
                    }
                    tasks::add(&repo, task)
                });
                (recipe, listed, hooks)
            },
            move |tab, (recipe, listed, hooks), ctx| {
                if let Some(Err(e)) = hooks {
                    tab.status = Some((
                        format!("The agent checkpoint hooks weren't installed: {e}"),
                        true,
                    ));
                }
                let recipe = recipe.unwrap_or_else(|e| {
                    tab.status = Some((format!("The worktree setup recipe wasn't run: {e}"), true));
                    Recipe::default()
                });
                if let Some(listed) = listed {
                    tab.tasks_written(listed);
                }
                let agent = made.agent.map(|(command, _)| command);
                if recipe.is_empty() && agent.is_none() {
                    return;
                }
                let launch = Launch {
                    label: agent
                        .as_deref()
                        .map_or_else(|| "setup".to_owned(), tasks::agent_label),
                    worktree: made.worktree,
                    recipe,
                    agent,
                    run: None,
                };
                if opened {
                    PendingLaunches::queue(launch);
                } else {
                    tab.start_launch(launch, ctx);
                }
            },
        );
    }

    fn start_agent_task(&mut self, new: start::NewTask, ctx: &egui::Context) {
        if self.busy_op.is_some() {
            self.status = Some(("Wait for the running operation to finish".to_owned(), true));
            return;
        }
        self.agents.dialog = None;
        let label = format!("Start agent task {}", new.branch);
        let worktree = worktree_path(&self.path, &new.folder);
        let task = AgentTask {
            branch: new.branch.clone(),
            path: worktree.display().to_string(),
            agent: new.agent.clone(),
            prompt: new.prompt.clone(),
            base: new.base.clone(),
            started: crate::git::now(),
        };
        let command = match crate::agent::AgentCli::of(&new.agent) {
            Some(cli) if new.agent.trim() == cli.program => cli.command(&new.prompt),
            _ => new.agent.clone(),
        };
        let ops = vec![
            [
                "worktree",
                "add",
                "-b",
                &new.branch,
                "--",
                &new.folder,
                &new.base,
            ]
            .map(str::to_owned)
            .to_vec(),
        ];
        self.agents.made = Some(Made {
            label: label.clone(),
            worktree: worktree.clone(),
            agent: Some((command, task)),
            hooks: new.hooks,
        });
        self.open_when_done = Some((label.clone(), worktree));
        self.run_ops(&label, ops, Refresh::All, OpKind::plain(), ctx);
    }

    fn open_recipe(&mut self, ctx: &egui::Context) {
        self.agents.dialog = Some(AgentDialog::Recipe(Box::new(RecipeForm::loading())));
        let repo = self.path.clone();
        self.then(
            ctx,
            move || Recipe::load(&repo),
            |tab, result, _| {
                if let Some(AgentDialog::Recipe(form)) = &mut tab.agents.dialog {
                    form.loaded(result);
                }
            },
        );
    }

    fn save_recipe(&mut self, recipe: Recipe, ctx: &egui::Context) {
        if let Some(AgentDialog::Recipe(form)) = &mut self.agents.dialog {
            form.saving = true;
        }
        let repo = self.path.clone();
        self.then(
            ctx,
            move || recipe.save(&repo),
            |tab, result, _| match result {
                Ok(()) => {
                    tab.agents.dialog = None;
                    tab.status = Some(("Worktree setup recipe saved".to_owned(), false));
                }
                Err(e) => {
                    if let Some(AgentDialog::Recipe(form)) = &mut tab.agents.dialog {
                        form.saving = false;
                        form.error = Some(e);
                    }
                }
            },
        );
    }
}

/// The folder `folder` (as git of the repository's machine was given it) in the form the app
/// gives paths: `ssh://host/…` for one elsewhere; here, relative to the repository.
fn worktree_path(repo: &std::path::Path, folder: &str) -> PathBuf {
    match crate::remote::parse(repo) {
        Some(target) if folder.starts_with('/') || folder.starts_with('~') => {
            crate::remote::encode(&target.host, folder)
        }
        Some(_) => crate::remote::join(repo, folder),
        None => repo.join(folder),
    }
}

#[cfg(test)]
mod tests;
