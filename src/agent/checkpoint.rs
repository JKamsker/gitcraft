//! Agent turns as review steps: after each turn of a coding agent, its hook runs
//! `gitcraft checkpoint`, which saves the worktree as it is (uncommitted and untracked files
//! included, its own index left alone) as a commit under `refs/gitcraft/agent/<session>/<n>`,
//! labelled with the prompt of that turn. _Uncommitted changes_ then shows the turns, each to
//! diff against the one before.

use std::path::Path;

use crate::git;

/// Where the turns are kept.
pub(crate) const REFS: &str = "refs/gitcraft/agent";

/// Who checkpoint commits are made by, whatever git is configured with in the agent's shell.
const AUTHOR: [(&str, &str); 4] = [
    ("GIT_AUTHOR_NAME", "gitcraft checkpoint"),
    ("GIT_AUTHOR_EMAIL", "checkpoint@gitcraft.invalid"),
    ("GIT_COMMITTER_NAME", "gitcraft checkpoint"),
    ("GIT_COMMITTER_EMAIL", "checkpoint@gitcraft.invalid"),
];

/// One saved turn.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Turn {
    pub session: String,
    pub number: u32,
    pub commit: String,
    /// The turn before it, or the commit checked out when the session's first turn began.
    pub parent: String,
    /// The first line of its prompt (`Turn n` without one).
    pub label: String,
    /// The whole prompt.
    pub prompt: String,
    /// When it was saved, in seconds since 1970.
    pub time: i64,
}

/// `session` as one component of a ref: letters, digits, `.`, `_` and `-`, at most 64 of them,
/// each byte of anything else a `-` (as `tr` makes it in the script for other machines).
pub(crate) fn session_name(session: &str) -> String {
    let mut name: String = session
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-') {
                char::from(b)
            } else {
                '-'
            }
        })
        .take(64)
        .collect();
    if name.is_empty() {
        name = "session".to_owned();
    }
    if name.starts_with('.') || name.ends_with('.') || name.ends_with(".lock") {
        name = format!("s{name}s");
    }
    name.replace("..", "-")
}

/// The commit message of turn `n`: its prompt's first line, the prompt, and trailers saying
/// whose turn it was. The script of [`super::checkpoint_sh`] writes the same.
fn message(prompt: &str, session: &str, top: &str, n: u32) -> String {
    let prompt = prompt.trim();
    let subject = prompt
        .lines()
        .next()
        .filter(|l| !l.trim().is_empty())
        .map_or_else(|| format!("Turn {n}"), |l| l.trim().to_owned());
    format!(
        "{subject}\n\n{prompt}\n\nGitcraft-Session: {session}\nGitcraft-Worktree: {top}\nGitcraft-Turn: {n}"
    )
}

/// Saves the worktree at `worktree` as the next turn of `session`, labelled `prompt`. `None`
/// when nothing changed since the session's last turn.
pub(crate) fn save(cwd: &Path, session: &str, prompt: &str) -> Result<Option<String>, String> {
    let session = session_name(session);
    let top = git::run(Some(cwd), &["rev-parse", "--show-toplevel"])?
        .trim_end_matches(['\n', '\r'])
        .to_owned();
    // From the top folder: the agent may run in a subfolder, where the paths git prints (the
    // temporary index's) are relative to it, but git reads them from the top.
    let worktree = &crate::remote::resolve(cwd, &top);
    let tree = super::snapshot::take(worktree)?;
    let last = numbers(worktree, &session)?.into_iter().max();
    let (parent, n) = match last {
        Some(last) => {
            let previous = git::run(
                Some(worktree),
                &["rev-parse", &format!("{REFS}/{session}/{last}")],
            )?
            .trim()
            .to_owned();
            let previous_tree = git::run(
                Some(worktree),
                &["rev-parse", &format!("{previous}^{{tree}}")],
            )?;
            if previous_tree.trim() == tree {
                return Ok(None);
            }
            (Some(previous), last + 1)
        }
        None => (
            git::run(Some(worktree), &["rev-parse", "--verify", "-q", "HEAD"])
                .ok()
                .map(|h| h.trim().to_owned()),
            1,
        ),
    };
    let msg = message(prompt, &session, &top, n);
    let mut args = vec!["commit-tree", tree.as_str()];
    if let Some(parent) = &parent {
        args.extend(["-p", parent.as_str()]);
    }
    args.extend(["-m", msg.as_str()]);
    let commit = git::run_env(Some(worktree), &args, &AUTHOR)?
        .trim()
        .to_owned();
    git::run(
        Some(worktree),
        &["update-ref", &format!("{REFS}/{session}/{n}"), &commit, ""],
    )?;
    Ok(Some(commit))
}

/// The turn numbers `session` has.
fn numbers(repo: &Path, session: &str) -> Result<Vec<u32>, String> {
    let out = git::run(
        Some(repo),
        &[
            "for-each-ref",
            "--format=%(refname)",
            &format!("{REFS}/{session}/"),
        ],
    )?;
    Ok(out
        .lines()
        .filter_map(|r| r.rsplit('/').next()?.parse().ok())
        .collect())
}

/// The turns saved in the worktree whose top folder git names `top` (as `--show-toplevel`
/// prints it), oldest first in each session, the session saved last first.
pub(crate) fn turns(repo: &Path, top: &str) -> Result<Vec<Turn>, String> {
    let out = git::run(
        Some(repo),
        &[
            "for-each-ref",
            "--format=%(refname)%1f%(objectname)%1f%(parent)%1f%(creatordate:unix)%1f%(contents)%1e",
            REFS,
        ],
    )?;
    let mut list: Vec<Turn> = out
        .split('\u{1e}')
        .filter_map(|record| parse_record(record.trim_start_matches('\n'), top))
        .collect();
    let latest = |session: &str, list: &[Turn]| {
        list.iter()
            .filter(|t| t.session == session)
            .map(|t| t.time)
            .max()
            .unwrap_or(0)
    };
    let order: Vec<(String, i64)> = list
        .iter()
        .map(|t| (t.session.clone(), latest(&t.session, &list)))
        .collect();
    let mut keyed: Vec<(i64, Turn)> = order
        .into_iter()
        .map(|(_, at)| at)
        .zip(list.drain(..))
        .collect();
    keyed.sort_by(|(a_at, a), (b_at, b)| {
        b_at.cmp(a_at)
            .then(a.session.cmp(&b.session))
            .then(a.number.cmp(&b.number))
    });
    Ok(keyed.into_iter().map(|(_, t)| t).collect())
}

fn parse_record(record: &str, top: &str) -> Option<Turn> {
    let mut fields = record.splitn(5, '\u{1f}');
    let refname = fields.next()?;
    let commit = fields.next()?.to_owned();
    let parent = fields
        .next()?
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_owned();
    let time = fields.next()?.trim().parse().unwrap_or(0);
    let contents = fields.next()?;
    let rest = refname.strip_prefix(REFS)?.strip_prefix('/')?;
    let (session, number) = rest.rsplit_once('/')?;
    let trailer = |name: &str| {
        contents
            .lines()
            .rev()
            .find_map(|l| l.strip_prefix(name)?.strip_prefix(": "))
            .map(str::to_owned)
    };
    if trailer("Gitcraft-Worktree").as_deref() != Some(top) {
        return None;
    }
    let (label, body) = contents.split_once('\n').unwrap_or((contents, ""));
    // At the trailers gitcraft wrote, the last ones: the prompt may say anything.
    let prompt = body
        .rsplit_once("\n\nGitcraft-Session: ")
        .map_or(body, |(prompt, _)| prompt)
        .trim()
        .to_owned();
    Some(Turn {
        session: session.to_owned(),
        number: number.parse().ok()?,
        commit,
        parent,
        label: label.trim().to_owned(),
        prompt,
        time,
    })
}

/// Deletes every turn of `session` (the commits go once git collects them).
pub(crate) fn forget(repo: &Path, session: &str) -> Result<(), String> {
    for n in numbers(repo, session)? {
        git::run(
            Some(repo),
            &["update-ref", "-d", &format!("{REFS}/{session}/{n}")],
        )?;
    }
    Ok(())
}

/// What the agent said this turn is, read from a hook's JSON.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct HookEvent {
    /// `UserPromptSubmit` (a prompt to remember), else a turn to save.
    pub event: String,
    pub session: String,
    pub cwd: String,
    pub prompt: String,
}

/// Claude Code's hook input (on stdin), or Codex's `notify` argument (`codex`).
pub(crate) fn parse_event(json: &str, codex: bool) -> Result<HookEvent, String> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("the hook's input isn't JSON: {e}"))?;
    let text = |key: &str| {
        value
            .get(key)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned()
    };
    if codex {
        let prompt = value
            .get("input-messages")
            .and_then(|m| m.as_array())
            .and_then(|m| m.last())
            .and_then(|m| m.as_str())
            .unwrap_or("")
            .to_owned();
        return Ok(HookEvent {
            event: text("type"),
            session: Some(text("thread-id"))
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "codex".to_owned()),
            cwd: text("cwd"),
            prompt,
        });
    }
    Ok(HookEvent {
        event: text("hook_event_name"),
        session: text("session_id"),
        cwd: text("cwd"),
        prompt: text("prompt"),
    })
}

/// The file the prompt of `session`'s turn under way is kept in until the turn is saved.
fn prompt_file(worktree: &Path, session: &str) -> Result<std::path::PathBuf, String> {
    super::repo_file(
        worktree,
        &format!("agent-prompts/{}", session_name(session)),
    )
}

/// Does what a hook asked: remembers the prompt, or saves the turn with it.
pub(crate) fn handle(event: &HookEvent, worktree: &Path) -> Result<Option<String>, String> {
    let file = prompt_file(worktree, &event.session)?;
    if event.event == "UserPromptSubmit" {
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        std::fs::write(&file, &event.prompt).map_err(|e| e.to_string())?;
        return Ok(None);
    }
    let prompt = if event.prompt.is_empty() {
        std::fs::read_to_string(&file).unwrap_or_default()
    } else {
        event.prompt.clone()
    };
    let saved = save(worktree, &event.session, &prompt)?;
    let _ = std::fs::remove_file(&file);
    Ok(saved)
}

/// `gitcraft checkpoint [--codex <json>]`, run by an agent's hook: Claude Code gives its JSON on
/// stdin, Codex's `notify` as the last argument. It never fails the agent: whatever goes wrong
/// is said on stderr, and it exits with 0. `None` when the arguments are for another command.
pub fn cli() -> Option<i32> {
    use std::io::IsTerminal;
    // Never `args()`, which panics on a path that isn't UTF-8 (a folder to open).
    let args: Vec<String> = std::env::args_os()
        .skip(1)
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    if args.first().map(String::as_str) != Some("checkpoint") {
        return None;
    }
    let codex = args.iter().any(|a| a == "--codex");
    // `gitcraft checkpoint` typed to open a folder of that name, not run by a hook (which gives
    // its JSON on stdin, or `--codex`): the window opens it.
    if !codex && (std::io::stdin().is_terminal() || Path::new("checkpoint").is_dir()) {
        return None;
    }
    let json = if codex {
        args.iter()
            .skip(1)
            .rfind(|a| !a.starts_with("--"))
            .cloned()
            .unwrap_or_default()
    } else {
        let mut text = String::new();
        let _ = std::io::Read::read_to_string(&mut std::io::stdin(), &mut text);
        text
    };
    let done = parse_event(&json, codex).and_then(|event| {
        let cwd = if event.cwd.is_empty() {
            std::env::current_dir().map_err(|e| e.to_string())?
        } else {
            std::path::PathBuf::from(&event.cwd)
        };
        handle(&event, &cwd)
    });
    if let Err(e) = done {
        eprintln!("gitcraft checkpoint: {e}");
    }
    Some(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sessions_become_ref_safe_names() {
        assert_eq!(session_name("2f1c-ab_9.x"), "2f1c-ab_9.x");
        assert_eq!(session_name("a b/c:ü"), "a-b-c---");
        assert_eq!(session_name(""), "session");
        assert_eq!(session_name(".hidden"), "s.hiddens");
        assert_eq!(session_name("x.lock"), "sx.locks");
        assert_eq!(session_name("a..b"), "a-b");
        assert_eq!(session_name(&"x".repeat(100)).len(), 64);
    }

    #[test]
    fn hook_inputs_are_read_for_both_agents() -> Result<(), String> {
        let claude = parse_event(
            r#"{"session_id":"s1","hook_event_name":"UserPromptSubmit","cwd":"/w","prompt":"fix ü \"x\""}"#,
            false,
        )?;
        assert_eq!(claude.prompt, "fix ü \"x\"");
        assert_eq!(claude.event, "UserPromptSubmit");
        let codex = parse_event(
            r#"{"type":"agent-turn-complete","thread-id":"t9","cwd":"/w","input-messages":["one","two"]}"#,
            true,
        )?;
        assert_eq!(
            (codex.session.as_str(), codex.prompt.as_str()),
            ("t9", "two")
        );
        assert!(parse_event("not json", false).is_err());
        Ok(())
    }

    #[test]
    fn a_turn_without_a_prompt_is_named_by_its_number() {
        assert!(message("", "s", "/w", 3).starts_with("Turn 3\n"));
        assert!(message("first\nsecond", "s", "/w", 1).starts_with("first\n\nfirst\nsecond\n\n"));
    }
}
