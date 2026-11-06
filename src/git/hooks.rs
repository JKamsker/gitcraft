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

/// The hooks `--no-verify` leaves out (of commit, merge and push).
const SKIPPABLE: [&str; 4] = ["pre-commit", "commit-msg", "pre-merge-commit", "pre-push"];

/// A hook that exited with something other than 0.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FailedHook {
    /// `pre-commit`, `pre-push`…
    pub name: String,
    pub code: i64,
}

impl FailedHook {
    /// The error of the command it failed, naming it first: what it said (`said`, possibly
    /// nothing) after.
    pub fn explain(&self, said: &str) -> String {
        let mut text = format!("the {} hook failed (exit code {})", self.name, self.code);
        let said = said.trim();
        if said.is_empty() {
            text.push_str(" and said nothing");
        } else {
            text.push_str(":\n");
            text.push_str(said);
        }
        if SKIPPABLE.contains(&self.name.as_str()) {
            text.push_str("\nSkip hooks (--no-verify) runs it once without this hook.");
        }
        text
    }
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
/// too, under their own `sid`, and only count when they are a hook's.
pub(super) fn failed_hook(events: &str) -> Option<FailedHook> {
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
                if let Some((_, _, name)) = hook.filter(|_| code != 0) {
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
    pub fn new() -> Self {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let name = format!(
            "gitcraft-hooks-{}-{}.json",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        );
        let path = std::env::temp_dir().join(name);
        // One left by an earlier gitcraft of the same process id would be read as this one's.
        let _ = std::fs::remove_file(&path);
        TraceFile(path)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    /// The hook that failed, as the events written say.
    pub fn failed_hook(&self) -> Option<FailedHook> {
        failed_hook(&std::fs::read_to_string(&self.0).ok()?)
    }
}

impl Drop for TraceFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// The script that runs `git <words>` (each word already quoted for `sh`) on a machine
/// elsewhere with its hook events written to a temporary file there, then passes them back
/// after its error output, behind [`MARK`]. Without a temporary file, git just runs.
pub(super) fn traced_script(words: &str) -> String {
    format!(
        concat!(
            "t=$(mktemp \"${{TMPDIR:-/tmp}}/gitcraft-hooks.XXXXXX\" 2>/dev/null) || t=\n",
            "if [ -n \"$t\" ]; then {var}=\"$t\" git {words}; else git {words}; fi\n",
            "r=$?\n",
            "if [ -n \"$t\" ]; then\n",
            "  printf '\\n%s\\n' '{mark}' >&2; grep -F '\"child_' \"$t\" >&2; rm -f \"$t\"\n",
            "fi\n",
            "exit $r",
        ),
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
            failed_hook(&events),
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
        assert_eq!(failed_hook(&events), None);
    }

    #[test]
    fn an_older_git_names_the_hook_by_its_program() {
        let events = concat!(
            r#"{"event":"child_start","sid":"S","child_id":4,"child_class":"hook","argv":["/my hooks/pre-push"]}"#,
            "\n",
            r#"{"event":"child_exit","sid":"S","child_id":4,"code":1}"#,
        );
        assert_eq!(
            failed_hook(events).map(|h| h.name),
            Some("pre-push".to_owned())
        );
    }

    #[test]
    fn the_explanation_names_the_hook_and_says_how_to_skip_it() {
        let hook = FailedHook {
            name: "pre-commit".into(),
            code: 1,
        };
        let text = hook.explain("  lint: 2 problems ✗\n");
        assert!(text.starts_with("the pre-commit hook failed (exit code 1):\nlint: 2 problems ✗"));
        assert!(text.contains("--no-verify"));
        let silent = FailedHook {
            name: "post-checkout".into(),
            code: 9,
        }
        .explain("");
        assert_eq!(
            silent,
            "the post-checkout hook failed (exit code 9) and said nothing"
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
        let out = std::process::Command::new("sh")
            .arg("-c")
            .arg(traced_script(&words.join(" ")))
            .current_dir(&dir)
            .output();
        let _ = std::fs::remove_dir_all(&dir);
        let Ok(out) = out else {
            return;
        };
        let (err, events) = split_trace(&out.stderr);
        assert_eq!(out.status.code(), Some(1));
        assert_eq!(String::from_utf8_lossy(&err).trim(), "nope ✗");
        assert_eq!(
            failed_hook(&events),
            Some(FailedHook {
                name: "pre-commit".into(),
                code: 4
            })
        );
    }
}
