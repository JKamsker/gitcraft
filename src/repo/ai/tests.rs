//! The commit box's AI button and the review panel, driven as the window drives them, against
//! real repositories and a fake tool (a script standing in for `claude -p`).

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::review::State;
use crate::ai::{AiSettings, AiTool, Answer, RunError};
use crate::git;
use crate::repo::RepoTab;
use crate::settings::Settings;

/// A repository with a commit, `topic` adding a file, and a staged change on `main`.
fn repo(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gitcraft ai tab ✓ {name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    let run = |args: &[&str]| assert!(git::run(Some(&dir), args).is_ok(), "{args:?}");
    run(&["init", "-q", "-b", "main"]);
    run(&["config", "user.name", "t"]);
    run(&["config", "user.email", "t@t"]);
    run(&["commit", "-q", "--allow-empty", "-m", "start"]);
    run(&["checkout", "-q", "-b", "topic"]);
    let _ = std::fs::write(dir.join("topic.txt"), "topic\n");
    run(&["add", "."]);
    run(&["commit", "-q", "-m", "topic work"]);
    run(&["checkout", "-q", "main"]);
    let _ = std::fs::write(dir.join("staged file.txt"), "staged\n");
    run(&["add", "."]);
    dir
}

/// The settings of `ctx` set to run `body` as the AI tool.
fn use_tool(ctx: &egui::Context, dir: &Path, body: &str) {
    let path = dir.join(".git").join("fake-tool.sh");
    let _ = std::fs::write(&path, format!("#!/bin/sh\n{body}\n"));
    let settings = Settings {
        ai: AiSettings {
            tool: AiTool::Custom,
            custom_command: format!("sh {}", crate::remote::sh_quote(&path.to_string_lossy())),
            ..AiSettings::default()
        },
        ..Settings::default()
    };
    settings.set(ctx);
}

/// Polls the tab as the window does until `done`, for a few seconds at most.
fn wait(tab: &mut RepoTab, ctx: &egui::Context, done: impl Fn(&RepoTab) -> bool) {
    let until = Instant::now() + Duration::from_secs(20);
    while !done(tab) && Instant::now() < until {
        tab.poll(ctx);
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn an_empty_box_gets_the_message() {
    let dir = repo("fill");
    let ctx = egui::Context::default();
    use_tool(
        &ctx,
        &dir,
        "cat >/dev/null\nprintf 'Add the staged file\\n\\nIt is staged.\\n'",
    );
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    tab.start_message(&ctx);
    wait(&mut tab, &ctx, |tab| tab.ai.writing.is_none());
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(tab.commit_summary, "Add the staged file");
    assert_eq!(tab.commit_body, "It is staged.");
    assert!(tab.ai.suggestion.is_none());
    assert_eq!(tab.ai.note, None);
}

#[test]
fn a_typed_message_is_kept_until_the_user_chooses() {
    let dir = repo("keep");
    let ctx = egui::Context::default();
    use_tool(&ctx, &dir, "cat >/dev/null\necho 'feat: theirs'");
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    tab.commit_body = "my description".to_owned();
    tab.start_message(&ctx);
    // Typed while the tool works.
    tab.commit_summary = "mine".to_owned();
    wait(&mut tab, &ctx, |tab| tab.ai.writing.is_none());
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(tab.commit_summary, "mine");
    assert_eq!(tab.commit_body, "my description");
    assert_eq!(
        tab.ai.suggestion,
        Some(("feat: theirs".to_owned(), String::new()))
    );
}

#[test]
fn a_cancelled_message_never_lands() {
    let dir = repo("cancel");
    let ctx = egui::Context::default();
    use_tool(&ctx, &dir, "cat >/dev/null\nsleep 1\necho 'too late'");
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    tab.start_message(&ctx);
    std::thread::sleep(Duration::from_millis(100));
    tab.cancel_message();
    let until = Instant::now() + Duration::from_secs(2);
    while Instant::now() < until {
        tab.poll(&ctx);
        std::thread::sleep(Duration::from_millis(20));
    }
    let _ = std::fs::remove_dir_all(&dir);
    assert!(tab.ai.writing.is_none());
    assert_eq!(tab.commit_summary, "");
    assert!(tab.ai.suggestion.is_none() && tab.ai.note.is_none());
}

#[test]
fn a_failure_is_said_under_the_box_and_an_old_answer_is_dropped() {
    let dir = repo("fail");
    let ctx = egui::Context::default();
    use_tool(
        &ctx,
        &dir,
        "cat >/dev/null\necho 'quota exceeded' >&2\nexit 1",
    );
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    tab.start_message(&ctx);
    wait(&mut tab, &ctx, |tab| tab.ai.writing.is_none());
    let _ = std::fs::remove_dir_all(&dir);
    let note = tab.ai.note.clone();
    assert!(
        note.as_ref().is_some_and(
            |(text, error)| *error && text.ends_with("exited with code 1: quota exceeded")
        ),
        "{note:?}"
    );
    // An answer to a request that is no longer the latest.
    tab.message_answered(
        999,
        Ok(Answer {
            text: "stale".to_owned(),
            files: 1,
            left_out: 0,
        }),
    );
    tab.message_answered(999, Err(RunError::Failed("stale".to_owned())));
    assert_eq!(tab.commit_summary, "");
    assert_eq!(tab.ai.note, note);
}

#[test]
fn a_review_shows_the_answer_without_remote_images() {
    let dir = repo("review");
    let ctx = egui::Context::default();
    let got = dir.join(".git").join("got.txt");
    use_tool(
        &ctx,
        &dir,
        &format!(
            "cat > {}\necho '## Fine'\necho '![tracker](https://evil.example/t.png)'",
            crate::remote::sh_quote(&got.to_string_lossy())
        ),
    );
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    tab.start_review(&ctx, "topic".to_owned(), "main".to_owned());
    wait(&mut tab, &ctx, |tab| {
        !matches!(
            tab.ai.review.as_ref().map(|r| &r.state),
            Some(State::Running(_))
        )
    });
    let prompt = std::fs::read_to_string(&got).unwrap_or_default();
    let _ = std::fs::remove_dir_all(&dir);
    let review = tab.ai.review.as_ref();
    let text = match review.map(|r| &r.state) {
        Some(State::Done(answer)) => answer.text.clone(),
        _ => String::new(),
    };
    assert!(text.starts_with("## Fine"), "{text}");
    assert!(!text.contains("!["), "{text}");
    assert!(prompt.contains("+++ b/topic.txt") && !prompt.contains("staged file"));
    assert_eq!(
        review.map(|r| (r.branch.as_str(), r.base.as_str())),
        Some(("topic", "main"))
    );
}

#[test]
fn a_review_without_a_tool_says_how_to_pick_one() {
    let dir = repo("review-off");
    let ctx = egui::Context::default();
    Settings::default().set(&ctx);
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    tab.start_review(&ctx, "topic".to_owned(), "main".to_owned());
    let _ = std::fs::remove_dir_all(&dir);
    assert!(matches!(
        tab.ai.review.as_ref().map(|r| &r.state),
        Some(State::Failed(e)) if e.starts_with("No AI tool is picked")
    ));
}

#[test]
fn closing_the_tab_stops_its_tool() {
    let dir = repo("drop");
    let ctx = egui::Context::default();
    let started = dir.join(".git").join("started");
    let pid = dir.join(".git").join("pid");
    use_tool(
        &ctx,
        &dir,
        &format!(
            "cat >/dev/null\nsleep 30 &\necho $! > {}\ntouch {}\nwait",
            crate::remote::sh_quote(&pid.to_string_lossy()),
            crate::remote::sh_quote(&started.to_string_lossy())
        ),
    );
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    tab.start_message(&ctx);
    let until = Instant::now() + Duration::from_secs(10);
    while !started.exists() && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(20));
    }
    drop(tab);
    let sleeper = std::fs::read_to_string(&pid).unwrap_or_default();
    // Stopped within a few seconds.
    let until = Instant::now() + Duration::from_secs(5);
    let mut alive = true;
    while alive && Instant::now() < until {
        let state = std::process::Command::new("ps")
            .args(["-o", "stat=", "-p", sleeper.trim()])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
            .unwrap_or_default();
        alive = !state.is_empty() && !state.starts_with('Z');
        std::thread::sleep(Duration::from_millis(50));
    }
    let _ = std::fs::remove_dir_all(&dir);
    assert!(!sleeper.trim().is_empty() && !alive, "{sleeper}");
}
