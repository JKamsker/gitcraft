//! The questions asked of a fake tool (a script standing in for `claude -p`) about real
//! repositories in folders with spaces and non-ASCII names: what it is given, what comes back,
//! stopping it, running out of time, and failures.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::*;
use crate::ai::{AiTool, TIMEOUT};
use crate::git;

/// A repository with a commit and a staged file, in a fresh folder named after `name`.
fn repo(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gitcraft ai ask ü {name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["commit", "-q", "--allow-empty", "-m", "feat: start"],
    ] {
        assert!(git::run(Some(&dir), args).is_ok(), "{args:?}");
    }
    let _ = std::fs::write(dir.join("grüße.txt"), "hallo welt\n");
    assert!(git::run(Some(&dir), &["add", "."]).is_ok());
    dir
}

/// A tool: a script in the repository's folder (so its path has spaces too), and the settings
/// that run it as the custom command.
fn tool(dir: &Path, body: &str) -> AiSettings {
    let path = dir.join("fake tool.sh");
    let _ = std::fs::write(&path, format!("#!/bin/sh\n{body}\n"));
    let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755));
    AiSettings {
        tool: AiTool::Custom,
        custom_command: crate::remote::sh_quote(&path.to_string_lossy()),
        ..AiSettings::default()
    }
}

fn ask_now(dir: &Path, settings: &AiSettings, timeout: Duration) -> Result<Answer, RunError> {
    ask(
        dir,
        settings,
        &Question::CommitMessage { amend: false },
        timeout,
        &Cancel::new(),
    )
}

#[test]
fn the_tool_gets_the_prompt_on_its_input_and_its_answer_comes_back() {
    let dir = repo("answer");
    let got = dir.join("got.txt");
    let settings = tool(
        &dir,
        &format!(
            "cat > {}\nprintf '\\033[32m```\\nfeat: greet in German\\n\\nSays hallo welt.\\n```\\n'",
            crate::remote::sh_quote(&got.to_string_lossy())
        ),
    );
    let answer = ask_now(&dir, &settings, TIMEOUT);
    let prompt = std::fs::read_to_string(&got).unwrap_or_default();
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(
        answer,
        Ok(Answer {
            text: "feat: greet in German\n\nSays hallo welt.".to_owned(),
            files: 1,
            left_out: 0,
        })
    );
    assert!(prompt.contains("+++ b/grüße.txt"), "{prompt}");
    assert!(prompt.contains("+hallo welt"));
    assert!(prompt.contains("Write a git commit message"));
}

#[test]
fn a_huge_change_reaches_the_tool_cut_to_size() {
    let dir = repo("huge");
    let big: String = (0..300_000).map(|i| format!("line {i}\n")).collect();
    let _ = std::fs::write(dir.join("big file.txt"), big);
    assert!(git::run(Some(&dir), &["add", "."]).is_ok());
    let mut settings = tool(&dir, "wc -c | tr -d ' '");
    settings.max_diff_kb = 32;
    let answer = ask_now(&dir, &settings, TIMEOUT);
    let _ = std::fs::remove_dir_all(&dir);
    let answer = answer.map_err(|e| e.message());
    let bytes: usize = answer
        .as_ref()
        .map(|a| a.text.parse().unwrap_or(usize::MAX))
        .unwrap_or(usize::MAX);
    assert!(bytes < 36 * 1024, "{answer:?}");
    assert_eq!(answer.map(|a| (a.files, a.left_out)), Ok((2, 1)));
}

#[test]
fn a_tool_that_reads_nothing_still_answers() {
    let dir = repo("unread");
    let big: String = (0..100_000).map(|i| format!("{i}\n")).collect();
    let _ = std::fs::write(dir.join("big.txt"), big);
    assert!(git::run(Some(&dir), &["add", "."]).is_ok());
    let mut settings = tool(&dir, "echo 'fix: x'");
    settings.max_diff_kb = 1024;
    let answer = ask_now(&dir, &settings, TIMEOUT);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(matches!(answer, Err(RunError::Failed(e)) if e.contains("kept its output open")));
}

#[test]
fn a_tool_can_be_stopped_with_what_it_started() {
    let dir = repo("cancel");
    let pid_file = dir.join("pid");
    let settings = tool(
        &dir,
        &format!(
            "cat >/dev/null\nsleep 30 &\necho $! > {}\nwait",
            crate::remote::sh_quote(&pid_file.to_string_lossy())
        ),
    );
    let cancel = Cancel::new();
    let stopper = {
        let (cancel, pid_file) = (cancel.clone(), pid_file.clone());
        std::thread::spawn(move || {
            let until = Instant::now() + Duration::from_secs(10);
            while !pid_file.exists() && Instant::now() < until {
                std::thread::sleep(Duration::from_millis(20));
            }
            std::thread::sleep(Duration::from_millis(50));
            cancel.cancel();
        })
    };
    let started = Instant::now();
    let answer = ask(
        &dir,
        &settings,
        &Question::CommitMessage { amend: false },
        TIMEOUT,
        &cancel,
    );
    let took = started.elapsed();
    let _ = stopper.join();
    let sleeper = std::fs::read_to_string(&pid_file).unwrap_or_default();
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(answer, Err(RunError::Cancelled));
    assert!(took < Duration::from_secs(10), "{took:?}");
    // What the tool started is gone too (or a zombie no one has reaped yet).
    let state = std::process::Command::new("ps")
        .args(["-o", "stat=", "-p", sleeper.trim()])
        .output()
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_owned())
        .unwrap_or_default();
    let alive = !state.is_empty() && !state.starts_with('Z');
    assert!(!sleeper.trim().is_empty() && !alive, "{sleeper}");
}

#[test]
fn a_tool_that_takes_too_long_is_stopped() {
    let dir = repo("timeout");
    let settings = tool(&dir, "cat >/dev/null\nsleep 30");
    let started = Instant::now();
    let answer = ask_now(&dir, &settings, Duration::from_millis(400));
    let took = started.elapsed();
    let _ = std::fs::remove_dir_all(&dir);
    let message = answer.map_err(|e| e.message());
    assert!(
        message
            .as_ref()
            .is_err_and(|e| e.ends_with("didn't answer within a second and was stopped.")),
        "{message:?}"
    );
    assert!(took < Duration::from_secs(10), "{took:?}");
}

#[test]
fn failures_are_explained() {
    let dir = repo("failures");
    let failed = ask_now(
        &dir,
        &tool(
            &dir,
            "cat >/dev/null\necho 'Not signed in: run login' >&2\nexit 4",
        ),
        TIMEOUT,
    );
    let silent = ask_now(
        &dir,
        &tool(&dir, "cat >/dev/null\nprintf '\\n  \\n'"),
        TIMEOUT,
    );
    let missing = ask_now(
        &dir,
        &AiSettings {
            tool: AiTool::Custom,
            custom_command: "gitcraft-no-such-tool-xyz --flag".to_owned(),
            ..AiSettings::default()
        },
        TIMEOUT,
    );
    let off = ask_now(&dir, &AiSettings::default(), TIMEOUT);
    let _ = git::run(Some(&dir), &["reset", "-q"]);
    let nothing_staged = ask_now(&dir, &tool(&dir, "echo never"), TIMEOUT);
    let _ = std::fs::remove_dir_all(&dir);
    let message =
        |r: Result<Answer, RunError>| r.map_err(|e| e.message()).err().unwrap_or_default();
    assert!(message(failed).ends_with("exited with code 4: Not signed in: run login"),);
    assert!(message(silent).ends_with("answered nothing."));
    assert!(
        message(missing).starts_with("gitcraft-no-such-tool-xyz isn't installed on this machine")
    );
    assert_eq!(message(off), "Pick a tool in Settings → AI first.");
    assert_eq!(
        message(nothing_staged),
        "Nothing is staged: stage the changes the message should describe."
    );
}

#[test]
fn a_review_asks_about_what_the_branch_adds() {
    let dir = repo("review");
    for args in [
        &["commit", "-q", "-m", "feat: greet"][..],
        &["checkout", "-q", "-b", "topic"],
    ] {
        assert!(git::run(Some(&dir), args).is_ok());
    }
    let _ = std::fs::write(dir.join("topic.txt"), "topic\n");
    for args in [&["add", "."][..], &["commit", "-q", "-m", "fix: topic"]] {
        assert!(git::run(Some(&dir), args).is_ok());
    }
    let got = dir.join("got.txt");
    let settings = tool(
        &dir,
        &format!(
            "cat > {}\necho '## Looks fine'",
            crate::remote::sh_quote(&got.to_string_lossy())
        ),
    );
    let question = Question::Review {
        branch: "topic".to_owned(),
        base: "main".to_owned(),
    };
    let answer = ask(&dir, &settings, &question, TIMEOUT, &Cancel::new());
    let prompt = std::fs::read_to_string(&got).unwrap_or_default();
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(answer.map(|a| a.text), Ok("## Looks fine".to_owned()));
    assert!(prompt.contains("branch topic adds to main"));
    assert!(prompt.contains("fix: topic"));
    assert!(prompt.contains("+++ b/topic.txt"));
    assert!(!prompt.contains("grüße"));
}

/// Delivered answer bytes cannot establish complete output while an inherited pipe
/// remains open. Report the explicit failure within the bounded drain/cleanup interval.
#[test]
fn inherited_open_output_is_reported_instead_of_a_complete_answer() {
    let dir = repo("linger");
    let settings = tool(&dir, "cat >/dev/null\necho 'fix: x'\nsleep 30 &");
    let started = Instant::now();
    let answer = ask_now(&dir, &settings, TIMEOUT);
    let took = started.elapsed();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(matches!(answer, Err(RunError::Failed(e)) if e.contains("kept its output open")));
    assert!(took < Duration::from_secs(10), "{took:?}");
}
