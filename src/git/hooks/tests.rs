use super::*;

const START: &str = r#"{"event":"child_start","sid":"S1","child_id":0,"child_class":"hook","hook_name":"pre-commit","argv":[".git/hooks/pre-commit"]}"#;

#[test]
fn the_failed_hook_is_named_with_its_exit_code() {
    let events = format!(
        "{START}\n{}\n",
        r#"{"event":"child_exit","sid":"S1","child_id":0,"code":3}"#
    );
    assert_eq!(
        failed_hook(&events, &["commit"]),
        Some(FailedHook {
            name: "pre-commit".into(),
            code: 3
        })
    );
}

/// A hook that passed is no failure, nor is a program that isn't a hook (a pager, gpg),
/// nor what a git run by the hook itself started (another `sid`).
#[test]
fn hooks_that_passed_and_other_programs_are_not_named() {
    let events = [
        START,
        r#"{"event":"child_exit","sid":"S1","child_id":0,"code":0}"#,
        r#"{"event":"child_start","sid":"S1","child_id":1,"child_class":"?","argv":["gpg"]}"#,
        r#"{"event":"child_exit","sid":"S1","child_id":1,"code":2}"#,
        r#"{"event":"child_exit","sid":"S1/S2","child_id":0,"code":1}"#,
        "not json",
        "",
    ]
    .join("\n");
    assert_eq!(failed_hook(&events, &["commit"]), None);
}

/// git goes on whatever `post-checkout` exits with inside a rebase, or `post-commit` in a
/// cherry-pick: a rebase stopped on a conflict is not that hook's doing.
#[test]
fn hooks_git_ignores_are_never_named() {
    let events = |name: &str| {
        format!(
            "{}\n{}",
            format_args!(
                r#"{{"event":"child_start","sid":"S","child_id":1,"child_class":"hook","hook_name":"{name}"}}"#
            ),
            r#"{"event":"child_exit","sid":"S","child_id":1,"code":2}"#
        )
    };
    assert_eq!(
        failed_hook(&events("post-checkout"), &["rebase", "main"]),
        None
    );
    assert_eq!(
        failed_hook(&events("post-commit"), &["cherry-pick", "a..b"]),
        None
    );
    assert_eq!(failed_hook(&events("post-merge"), &["pull"]), None);
    let checkout = failed_hook(&events("post-checkout"), &["checkout", "main", "--"]);
    assert_eq!(checkout.map(|h| h.code), Some(2));
}

#[test]
fn an_older_git_names_the_hook_by_its_program() {
    let events = concat!(
        r#"{"event":"child_start","sid":"S","child_id":4,"child_class":"hook","argv":["/my hooks/pre-push"]}"#,
        "\n",
        r#"{"event":"child_exit","sid":"S","child_id":4,"code":1}"#,
    );
    assert_eq!(
        failed_hook(events, &["push"]).map(|h| h.name),
        Some("pre-push".to_owned())
    );
}

#[test]
fn the_explanation_names_the_hook_and_says_how_to_skip_it() {
    let hook = FailedHook {
        name: "pre-commit".into(),
        code: 1,
    };
    let text = hook.explain("  lint: 2 problems ✗\n", &["commit", "-m", "x"]);
    assert_eq!(
        text,
        "the pre-commit hook stopped it (exit code 1)\nlint: 2 problems ✗\n\
         Tick Skip hooks in the commit box to commit once without them."
    );
    let push = FailedHook {
        name: "pre-push".into(),
        code: 1,
    };
    let args = ["-c", "push.default=simple", "push", "-u", "o", "main"];
    assert!(
        push.explain("", &args)
            .ends_with("to push once without it.")
    );
    // A merge's commit-msg can't be skipped from the app: no hint.
    let merge = FailedHook {
        name: "commit-msg".into(),
        code: 2,
    };
    assert_eq!(
        merge.explain("", &["merge", "x"]),
        "the commit-msg hook stopped it (exit code 2)"
    );
    let after = FailedHook {
        name: "post-checkout".into(),
        code: 9,
    }
    .explain("", &["checkout", "main", "--"]);
    assert_eq!(
        after,
        "it was done, but the post-checkout hook failed afterwards (exit code 9)"
    );
}

#[test]
fn only_commands_that_run_hooks_are_traced() {
    assert!(wants_trace(&["commit", "-m", "x"]));
    assert!(wants_trace(&[
        "-c",
        "push.default=simple",
        "push",
        "-u",
        "o"
    ]));
    assert!(wants_trace(&["--literal-pathspecs", "checkout", "--", "a"]));
    assert!(!wants_trace(&["add", "-A"]));
    assert!(!wants_trace(&["-c", "commit", "status"]));
    assert!(!wants_trace(&[]));
}

#[test]
fn the_events_are_split_off_the_error_output() {
    let stderr = format!("oops ✗\n\n{MARK}\n{START}\n");
    let (err, events) = split_trace(stderr.as_bytes());
    assert_eq!(String::from_utf8_lossy(&err), "oops ✗\n");
    assert_eq!(events.trim(), START);
    let (plain, none) = split_trace(b"no events");
    assert_eq!((plain.as_slice(), none.as_str()), (&b"no events"[..], ""));
}

/// The script run for a repository elsewhere (in WSL, over SSH), run here by `sh`: the
/// command's error output comes back as it was, the hook's events after it.
#[cfg(unix)]
#[test]
fn the_script_for_elsewhere_passes_the_hook_events_back() {
    let dir = std::env::temp_dir().join(format!("gitcraft-hook-sh ✓-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    let git = |args: &[&str]| super::super::run(Some(&dir), args);
    for args in [
        &["init", "-q"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
    ] {
        assert!(git(args).is_ok(), "{args:?}");
    }
    let hook = dir.join(".git/hooks/pre-commit");
    let _ = std::fs::write(&hook, "#!/bin/sh\necho 'nope ✗' >&2\nexit 4\n");
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755));
    let words: Vec<String> = ["commit", "--allow-empty", "-m", "it's ünï"]
        .into_iter()
        .map(crate::remote::sh_quote)
        .collect();
    let shell_temp = dir.join(".git").join("shell temp ✓");
    let _ = std::fs::create_dir_all(&shell_temp);
    let out = std::process::Command::new("sh")
        .arg("-c")
        .arg(traced_script(&words.join(" ")))
        .env("T", &shell_temp)
        .current_dir(&dir)
        .output();
    let left = std::fs::read_dir(&shell_temp).map_or(0, |d| d.count());
    // Without the shell's folder, git just runs: the command's error output alone.
    let untraced = std::process::Command::new("sh")
        .arg("-c")
        .arg(traced_script(&words.join(" ")))
        .env_remove("T")
        .current_dir(&dir)
        .output();
    let _ = std::fs::remove_dir_all(&dir);
    let Ok(out) = out else {
        return;
    };
    let (err, events) = split_trace(&out.stderr);
    assert_eq!(left, 0, "the events file was left behind");
    let untraced = untraced.map_or(String::new(), |o| {
        String::from_utf8_lossy(&o.stderr).into_owned()
    });
    assert_eq!(untraced.trim(), "nope ✗");
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(String::from_utf8_lossy(&err).trim(), "nope ✗");
    assert_eq!(
        failed_hook(&events, &["commit", "--allow-empty"]),
        Some(FailedHook {
            name: "pre-commit".into(),
            code: 4
        })
    );
}
