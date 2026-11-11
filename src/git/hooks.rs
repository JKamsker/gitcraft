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
    if !STOPPING.contains(&name) && name != "post-checkout" {
        return None;
    }
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
    super::parse_command_envelope(args)
        .ok()
        .map(|(at, _)| args[at])
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
mod tests;

#[cfg(test)]
mod backend_fluent_identity_tests;
