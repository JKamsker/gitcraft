//! Which hook made a git command fail. git's trace2 events name every hook it runs and how it
//! exited; they are written to a file of their own (`GIT_TRACE2_EVENT=<file>`), so the hooks'
//! own output and error output are left exactly as they are.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// The variable git writes its trace2 events to the file named by.
pub(super) const TRACE_VAR: &str = "GIT_TRACE2_EVENT";

/// Between a command's error output and the hook events, from a repository elsewhere (see
/// [`traced_script`]).
const MARK: &str = "@@gitcraft-hook-events@@";

/// The git commands that run hooks whose failure fails them: only those are traced.
const HOOKED: [&str; 10] = [
    "commit",
    "push",
    "merge",
    "pull",
    "cherry-pick",
    "revert",
    "rebase",
    "am",
    "checkout",
    "switch",
];

/// The hooks whose failure stops the command running them. git goes on whatever the others
/// (`post-commit`, `post-merge`, `post-rewrite`…) exit with, so they never are why it failed.
const STOPPING: [&str; 8] = [
    "pre-commit",
    "prepare-commit-msg",
    "commit-msg",
    "pre-merge-commit",
    "pre-push",
    "pre-rebase",
    "applypatch-msg",
    "pre-applypatch",
];

/// Whether the hook `name` failing makes the git command `args` fail: one of [`STOPPING`], or
/// `post-checkout` of a checkout itself (git ignores it inside a rebase or a clone).
fn can_fail(name: &str, args: &[&str]) -> bool {
    STOPPING.contains(&name)
        || (name == "post-checkout" && matches!(subcommand(args), Some("checkout" | "switch")))
}

/// A hook that exited with something other than 0.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FailedHook {
    /// `pre-commit`, `pre-push`…
    pub name: String,
    pub code: i64,
}

impl FailedHook {
    /// The error of the git command `args` it failed: a first line naming the hook, then what
    /// it printed (`said`, possibly nothing), then how to run it once without, where the app
    /// can.
    pub fn explain(&self, said: &str, args: &[&str]) -> String {
        let (name, code) = (&self.name, self.code);
        // A `post-` hook runs once the work is done: only its exit code fails the command.
        let mut text = if name.starts_with("post-") {
            format!("it was done, but the {name} hook failed afterwards (exit code {code})")
        } else {
            format!("the {name} hook stopped it (exit code {code})")
        };
        let said = said.trim();
        if !said.is_empty() {
            text.push('\n');
            text.push_str(said);
        }
        let hint = match (subcommand(args), name.as_str()) {
            (Some("commit"), "pre-commit" | "commit-msg") => {
                Some("Tick Skip hooks in the commit box to commit once without them.")
            }
            (Some("push"), "pre-push") => {
                Some("Right-click Push for Push, skipping hooks, to push once without it.")
            }
            _ => None,
        };
        if let Some(hint) = hint {
            text.push('\n');
            text.push_str(hint);
        }
        text
    }
}

/// Render only the application-owned hook envelope. The hook's own output stays opaque.
pub(super) fn display_failure(error: &str) -> Option<String> {
    let (header, detail) = error.split_once('\n').unwrap_or((error, ""));
    let (rest, marker, id) = if let Some(rest) = header.strip_prefix("it was done, but the ") {
        (
            rest,
            " hook failed afterwards (exit code ",
            "backend-it-was-done-but-the-name-hook-failed-afterwards-exit-code-code-ce81ed",
        )
    } else {
        (
            header.strip_prefix("the ")?,
            " hook stopped it (exit code ",
            "backend-the-name-hook-stopped-it-exit-code-code-cc1a4c",
        )
    };
    let (name, code) = rest.strip_suffix(')')?.rsplit_once(marker)?;
    code.parse::<i64>().ok()?;
    let mut display = crate::i18n::message(id, &[("name", name.into()), ("code", code.into())]);
    if !detail.is_empty() {
        let (said, last) = detail.rsplit_once('\n').unwrap_or(("", detail));
        let hint_id = match last {
            "Tick Skip hooks in the commit box to commit once without them." => {
                Some("backend-tick-skip-hooks-in-the-commit-box-to-commit-once-without-them-cab006")
            }
            "Right-click Push for Push, skipping hooks, to push once without it." => Some(
                "backend-right-click-push-for-push-skipping-hooks-to-push-once-without-it-4b9ac1",
            ),
            _ => None,
        };
        display.push('\n');
        if let Some(hint_id) = hint_id {
            if !said.is_empty() {
                display.push_str(said);
                display.push('\n');
            }
            display.push_str(crate::i18n::text(hint_id));
        } else {
            display.push_str(detail);
        }
    }
    Some(display)
}

/// Whether the git command `args` (options before the subcommand allowed) is one whose hooks
/// are traced.
pub(super) fn wants_trace(args: &[&str]) -> bool {
    subcommand(args).is_some_and(|sub| HOOKED.contains(&sub))
}

/// The subcommand of `args`: the first word that isn't an option before it (`-c x=y` and
/// `-C dir` take the word after them).
pub fn subcommand<'a>(args: &[&'a str]) -> Option<&'a str> {
    let mut words = args.iter().copied();
    while let Some(word) = words.next() {
        match word {
            "-c" | "-C" => {
                words.next();
            }
            w if w.starts_with('-') => {}
            w => return Some(w),
        }
    }
    None
}

/// The last hook that failed, from trace2 events (one JSON object a line). A hook git starts
/// is a `child_start` of the class `hook`; its `child_exit`, of the same process (`sid`) and
/// child, has its exit code. Events of the git commands a hook runs itself are in the file
/// too, under their own `sid`, and only count when they are a hook's. Only a hook whose
/// failure fails the git command `args` (see [`can_fail`]) counts.
pub(super) fn failed_hook(events: &str, args: &[&str]) -> Option<FailedHook> {
    let mut started: Vec<(String, i64, String)> = Vec::new();
    let mut failed = None;
    for line in events.lines() {
        let Ok(event) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let text = |key: &str| event.get(key).and_then(|v| v.as_str()).unwrap_or_default();
        let (sid, child) = (text("sid"), event.get("child_id").and_then(|v| v.as_i64()));
        let Some(child) = child else { continue };
        match text("event") {
            "child_start" if text("child_class") == "hook" => {
                started.push((sid.to_owned(), child, hook_name(&event)));
            }
            "child_exit" => {
                let code = event.get("code").and_then(|v| v.as_i64()).unwrap_or(0);
                let hook = started.iter().find(|(s, c, _)| s == sid && *c == child);
                if let Some((_, _, name)) = hook.filter(|(_, _, n)| code != 0 && can_fail(n, args))
                {
                    failed = Some(FailedHook {
                        name: name.clone(),
                        code,
                    });
                }
            }
            _ => {}
        }
    }
    failed
}

/// The name of the hook a `child_start` starts: `hook_name`, else (older gits) the file name
/// of the program run.
fn hook_name(event: &serde_json::Value) -> String {
    if let Some(name) = event.get("hook_name").and_then(|v| v.as_str()) {
        return name.to_owned();
    }
    let program = event
        .get("argv")
        .and_then(|v| v.get(0))
        .and_then(|v| v.as_str())
        .unwrap_or("a");
    let name = program.rsplit(['/', '\\']).next().unwrap_or(program);
    name.to_owned()
}

/// A file of its own for the trace2 events of one command run here, removed once read (or
/// dropped).
pub(super) struct TraceFile(PathBuf);

impl TraceFile {
    /// A new, empty file only this user can read (the events name the command run, a commit's
    /// message with it), made here rather than by git: one someone else made first under the
    /// name is never taken. `None` when none could be made: the command runs untraced.
    pub fn new() -> Option<Self> {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.subsec_nanos());
        for _ in 0..4 {
            let name = format!(
                "gitcraft-hooks-{}-{nanos:x}-{}.json",
                std::process::id(),
                SEQ.fetch_add(1, Ordering::Relaxed)
            );
            let path = std::env::temp_dir().join(name);
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            if options.open(&path).is_ok() {
                return Some(TraceFile(path));
            }
        }
        None
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    /// The hook that failed the git command `args`, as the events written say.
    pub fn failed_hook(&self, args: &[&str]) -> Option<FailedHook> {
        failed_hook(&std::fs::read_to_string(&self.0).ok()?, args)
    }
}

impl Drop for TraceFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// The script that runs `git <words>` (each word already quoted for `sh`) on a machine
/// elsewhere with its hook events written to a file in the shell's own temporary folder (`$T`,
/// always absolute, removed with the shell however it ends), then passes them back after its
/// error output, behind [`MARK`]. Without that folder, git just runs. Its first line names the
/// command, for messages about it (`Request::what`).
#[cfg(all(test, unix))]
pub(super) fn traced_script(words: &str) -> String {
    traced_script_with_program(words, "git")
}
/// The fixed selected executable for credential-bearing remote runners.
pub(super) fn traced_script_with_program(words: &str, program: &str) -> String {
    format!(
        concat!(
            ": {program} {words}\n",
            "t=\"${{T:+$T/hook-events}}\"\n",
            "if [ -n \"$t\" ] && : >\"$t\" 2>/dev/null; then {var}=\"$t\" {program} {words}\n",
            "else t=; {program} {words}; fi\n",
            "r=$?\n",
            "if [ -n \"$t\" ]; then\n",
            "  printf '\\n%s\\n' '{mark}' >&2; grep -F '\"child_' \"$t\" >&2; rm -f \"$t\"\n",
            "fi\n",
            "exit $r",
        ),
        program = program,
        var = TRACE_VAR,
        words = words,
        mark = MARK,
    )
}

/// The error output of a [`traced_script`], and the hook events after it.
pub(super) fn split_trace(stderr: &[u8]) -> (Vec<u8>, String) {
    let mark = format!("\n{MARK}\n");
    let found = stderr
        .windows(mark.len())
        .rposition(|w| w == mark.as_bytes());
    match found {
        Some(at) => (
            stderr[..at].to_vec(),
            String::from_utf8_lossy(&stderr[at + mark.len()..]).into_owned(),
        ),
        None => (stderr.to_vec(), String::new()),
    }
}

#[cfg(test)]
mod tests {
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
}

#[cfg(test)]
mod backend_fluent_identity_tests {
    #[test]
    fn backend_fluent_hook_keeps_toolbar_header_and_opaque_output() -> Result<(), String> {
        let said =
            "provider HTTP 409: the other hook stopped it (exit code 7)\n/refs/heads/de/main";
        for locale in ["en-US", "de"] {
            crate::i18n::set_language(locale);
            for (name, args, prefix) in [
                ("pre-commit", vec!["commit"], "the "),
                ("pre-push", vec!["push"], "the "),
                ("post-checkout", vec!["checkout"], "it was done, but the "),
            ] {
                let hook = super::FailedHook {
                    name: name.into(),
                    code: 1234,
                };
                let raw = hook.explain(said, &args);
                let header = raw.lines().next().ok_or("missing hook header")?;
                assert!(header.starts_with(prefix) && header.contains(" hook "));
                assert!(raw.contains("exit code 1234)"));
                assert!(raw.contains(said));
                let display = crate::git::display_error(&raw);
                assert!(display.contains(said));
                if locale == "de" {
                    assert!(display.contains("Hook") && display.contains("Exit-Code 1234"));
                    assert_ne!(display, raw);
                } else {
                    assert_eq!(display, raw);
                }
            }
            let opaque = "fatal: provider hook output";
            assert_eq!(crate::git::display_error(opaque), opaque);
        }
        crate::i18n::set_language("en-US");
        Ok(())
    }
}
