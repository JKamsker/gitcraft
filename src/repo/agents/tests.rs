//! Agent tasks and the setup recipe through the tab, the way the dialogs drive them, on real
//! repositories: the worktree is made, the task listed, the recipe run in a real terminal.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::start::NewTask;
use super::{AgentAct, worktree_path};
use crate::agent::{PendingLaunches, Recipe, tasks};
use crate::git;
use crate::repo::{Action, OpenRequest, Refresh, RepoTab};

/// A repository with one commit and an ignored `.env.local`, in a folder whose name has a space
/// and a letter outside ASCII.
fn repo(name: &str) -> PathBuf {
    let root =
        std::env::temp_dir().join(format!("gitcraft-agents-{name}-{} é", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let dir = root.join("app");
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(dir.join(".gitignore"), ".env.local\n");
    let _ = std::fs::write(dir.join(".env.local"), "TOKEN=x\n");
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["config", "commit.gpgSign", "false"],
        &["add", "-A"],
        &["commit", "-q", "-m", "base"],
    ] {
        assert!(git::run(Some(&dir), args).is_ok(), "{args:?}");
    }
    dir
}

/// Polls the tab until `done` holds or a while has passed.
fn pump(tab: &mut RepoTab, ctx: &egui::Context, done: impl Fn(&RepoTab) -> bool) -> bool {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(30) {
        tab.poll(ctx);
        if done(tab) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

#[test]
fn a_new_folder_is_found_where_git_puts_it() {
    assert_eq!(
        worktree_path(Path::new("ssh://h/home/me/app"), "/home/me/app-x"),
        PathBuf::from("ssh://h/home/me/app-x")
    );
    assert_eq!(
        worktree_path(Path::new("/w/app"), "../app-x"),
        PathBuf::from("/w/app/../app-x")
    );
    let absolute = std::env::temp_dir().join("app-x");
    assert_eq!(
        worktree_path(Path::new("/w/app"), &absolute.display().to_string()),
        absolute
    );
}

#[test]
fn starting_a_task_makes_its_worktree_lists_it_and_hands_its_tab_the_agent() {
    let dir = repo("start");
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    let recipe = Recipe {
        copy: vec![".env.local".to_owned()],
        ..Recipe::default()
    };
    assert!(recipe.save(&dir).is_ok());
    let folder = dir.with_file_name("app-agent-fix ü");
    tab.apply(
        Action::Agent(AgentAct::Start(Box::new(NewTask {
            branch: "agent/fix-ü".to_owned(),
            folder: folder.display().to_string(),
            base: "main".to_owned(),
            agent: "claude".to_owned(),
            prompt: "fix the 'login' bug".to_owned(),
            hooks: true,
        }))),
        &ctx,
    );
    let listed = pump(&mut tab, &ctx, |tab| !tab.agents.tasks.is_empty());
    let opens = matches!(&tab.open_request, Some(OpenRequest::Worktree(p)) if *p == folder);
    // The worktree's tab takes the launch: the recipe, then the agent with its first message.
    // As a tab of it asks: by its folder resolved (on Windows, not `RUNNER~1`'s short form).
    let launch = PendingLaunches::take(&crate::agent::launch::tab_form(&folder));
    let task = tab.agents.tasks.first().cloned().unwrap_or_default();
    let on_disk = tasks::load(&dir).unwrap_or_default();
    let branch = git::run(Some(&folder), &["branch", "--show-current"]).unwrap_or_default();
    let base = git::head(&dir);
    let claude =
        std::fs::read_to_string(folder.join(".claude/settings.local.json")).unwrap_or_default();
    let exclude = std::fs::read_to_string(dir.join(".git/info/exclude")).unwrap_or_default();
    let clean = git::run(Some(&folder), &["status", "--porcelain"]).unwrap_or_default();
    let _ = std::fs::remove_dir_all(dir.parent().unwrap_or(&dir));
    assert!(listed, "{:?} {:?}", tab.status, tab.agents.error);
    assert!(opens);
    assert_eq!(branch.trim(), "agent/fix-ü");
    assert_eq!(on_disk, tab.agents.tasks);
    assert_eq!(task.base, base, "the base is kept as a commit");
    assert_eq!(task.prompt, "fix the 'login' bug");
    assert!(launch.is_some(), "no launch for {}", folder.display());
    let launch = launch.unwrap_or_else(|| super::Launch {
        label: String::new(),
        worktree: PathBuf::new(),
        recipe: Recipe::default(),
        agent: None,
        run: None,
    });
    assert_eq!(launch.recipe.copy, [".env.local"]);
    assert_eq!(
        launch.agent.as_deref(),
        Some(r"claude 'fix the '\''login'\'' bug'")
    );
    assert_eq!(launch.label, "Claude Code");
    // The hooks that save each turn, kept out of git.
    assert!(
        claude.contains("UserPromptSubmit") && claude.contains("checkpoint"),
        "{claude}"
    );
    assert!(
        exclude.contains("/.claude/settings.local.json"),
        "{exclude}"
    );
    assert_eq!(clean, "", "the hooks' files show as no change");
}

/// Any worktree made from the tab gets the recipe, run in a real terminal of the tab when the
/// worktree isn't opened: the file it copies arrives.
#[test]
fn a_worktree_added_from_the_tab_is_set_up_in_a_terminal() {
    let dir = repo("added");
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    let recipe = Recipe {
        copy: vec![".env.local".to_owned()],
        command: "echo ran > setup-ran.txt".to_owned(),
        ..Recipe::default()
    };
    assert!(recipe.save(&dir).is_ok());
    let folder = dir.with_file_name("plain wt");
    let path = folder.display().to_string();
    tab.apply(
        Action::Ops {
            label: "Add worktree".to_owned(),
            ops: vec![
                ["worktree", "add", "-b", "feat", &path, "HEAD"]
                    .map(str::to_owned)
                    .to_vec(),
            ],
            refresh: Refresh::All,
        },
        &ctx,
    );
    let started = pump(&mut tab, &ctx, |tab| {
        !tab.terminals.is_empty() || tab.terminal_error.is_some()
    });
    let arrived = pump(&mut tab, &ctx, |_| folder.join("setup-ran.txt").is_file());
    let copied = std::fs::read_to_string(folder.join(".env.local")).unwrap_or_default();
    let error = tab.terminal_error.clone();
    tab.terminals.clear();
    let _ = std::fs::remove_dir_all(dir.parent().unwrap_or(&dir));
    assert!(started && error.is_none(), "{error:?} {:?}", tab.status);
    assert!(arrived, "the setup command didn't run");
    assert_eq!(copied, "TOKEN=x\n");
}

#[test]
fn a_worktree_made_without_a_recipe_opens_no_terminal() {
    let dir = repo("no-recipe");
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    let folder = dir.with_file_name("bare wt");
    let path = folder.display().to_string();
    tab.apply(
        Action::Ops {
            label: "Add worktree".to_owned(),
            ops: vec![
                ["worktree", "add", &path, "-b", "x"]
                    .map(str::to_owned)
                    .to_vec(),
            ],
            refresh: Refresh::All,
        },
        &ctx,
    );
    let done = pump(&mut tab, &ctx, |tab| {
        tab.busy_op.is_none() && tab.agents.made.is_none()
    });
    // The recipe's read comes back a moment after the operation.
    for _ in 0..25 {
        tab.poll(&ctx);
        std::thread::sleep(Duration::from_millis(20));
    }
    let exists = folder.is_dir();
    let _ = std::fs::remove_dir_all(dir.parent().unwrap_or(&dir));
    assert!(done && exists, "{:?}", tab.status);
    assert!(tab.terminals.is_empty());
}
