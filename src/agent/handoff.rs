//! Hand it to the agent: what gitcraft types into a worktree's agent terminal, as one message.
//! Line notes written on any diff (kept in a tray until sent, never published anywhere), a
//! conflict's three sides, the log of a failing CI run, a pull request's thread.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// A note on a line of a diff, for the agent.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Note {
    pub path: String,
    pub line: u64,
    /// On the line as it was (a removed line), not as it is.
    pub old: bool,
    /// The line itself, so the agent finds it even after its number moved.
    pub code: String,
    pub text: String,
    /// Which diff it was written on ("Unstaged changes", "Commit 1a2b3c4d"…).
    pub source: String,
}

/// At most this much of a CI log goes to the agent: its end, where the failure is.
const LOG_LINES: usize = 200;
const LOG_CHARS: usize = 16_000;

/// The notes as one message.
pub(crate) fn notes_message(notes: &[Note]) -> String {
    let mut msg = String::from("Review notes. Please address each one:\n");
    for (i, note) in notes.iter().enumerate() {
        let mut about: Vec<&str> = Vec::new();
        if !note.source.is_empty() {
            about.push(&note.source);
        }
        if note.old {
            about.push("a removed line");
        }
        let about = if about.is_empty() {
            String::new()
        } else {
            format!(" ({})", about.join(", "))
        };
        msg.push_str(&format!(
            "\n{}. {}:{}{about}\n",
            i + 1,
            note.path,
            note.line
        ));
        if !note.code.trim().is_empty() {
            msg.push_str(&format!("   > {}\n", note.code.trim_end()));
        }
        for line in note.text.trim().lines() {
            msg.push_str(&format!("   {line}\n"));
        }
    }
    msg
}

/// One conflict of a file: (ours, the base when the file has it, theirs).
pub(crate) type Sides<'a> = (&'a str, Option<&'a str>, &'a str);

/// A file's conflicts as one message: each with its three sides.
pub(crate) fn conflict_message(
    path: &str,
    ours: &str,
    theirs: &str,
    conflicts: &[Sides],
) -> String {
    let mut msg = format!(
        "{path} has {} merge conflict{}. Resolve them in the file, keeping what each side meant, \
         and remove the conflict markers:\n",
        conflicts.len(),
        if conflicts.len() == 1 { "" } else { "s" }
    );
    for (i, (o, base, t)) in conflicts.iter().enumerate() {
        msg.push_str(&format!("\nConflict {}:\n", i + 1));
        msg.push_str(&block(&format!("ours ({ours})"), o));
        if let Some(base) = base {
            msg.push_str(&block("base (what both started from)", base));
        }
        msg.push_str(&block(&format!("theirs ({theirs})"), t));
    }
    msg
}

/// The failing run's log, its end at most, as one message.
pub(crate) fn ci_message(run: &str, log: &str) -> String {
    let lines: Vec<&str> = log.lines().collect();
    let start = lines.len().saturating_sub(LOG_LINES);
    let mut excerpt = lines[start..].join("\n");
    if excerpt.chars().count() > LOG_CHARS {
        let skip = excerpt.chars().count() - LOG_CHARS;
        excerpt = excerpt.chars().skip(skip).collect();
    }
    let cut = if start > 0 || excerpt.len() < log.trim_end().len() {
        " (its last lines)"
    } else {
        ""
    };
    format!(
        "The CI run \"{run}\" failed. Find out why and fix it. Its log{cut}:\n\n{}",
        fenced(&excerpt)
    )
}

/// A review thread of a pull request, with its file and line, as one message.
pub(crate) fn thread_message(
    path: &str,
    line: Option<u64>,
    comments: &[(String, String)],
) -> String {
    let place = line.map_or_else(|| path.to_owned(), |l| format!("{path}:{l}"));
    let mut msg = format!("A review thread on {place}. Please address it:\n");
    for (author, body) in comments {
        msg.push_str(&format!("\n{author}:\n"));
        for l in body.trim().lines() {
            msg.push_str(&format!("  {l}\n"));
        }
    }
    msg
}

fn block(title: &str, text: &str) -> String {
    format!("{title}:\n{}\n", fenced(text.trim_end_matches('\n')))
}

/// `text` in a Markdown code fence longer than any run of backticks in it.
fn fenced(text: &str) -> String {
    let mut longest = 0;
    let mut run = 0;
    for c in text.chars() {
        if c == '`' {
            run += 1;
            longest = longest.max(run);
        } else {
            run = 0;
        }
    }
    let fence = "`".repeat(longest.max(2) + 1);
    format!("{fence}\n{text}\n{fence}")
}

/// Which terminal a message goes to: the one picked (while it still is the terminal of that
/// name), else the newest one an agent was started in, by its tab's name, whose program
/// hasn't ended. Never just any terminal: a shell would take the paste as a command to run.
pub(crate) fn pick_terminal(
    terminals: &[(&str, bool)],
    agents: &[String],
    picked: Option<(usize, &str)>,
) -> Option<usize> {
    if let Some((i, name)) = picked
        && terminals
            .get(i)
            .is_some_and(|(label, ended)| *label == name && !ended)
    {
        return Some(i);
    }
    terminals
        .iter()
        .rposition(|(label, ended)| !ended && agents.iter().any(|a| a == label))
}

/// The tray of a worktree, kept in its own git folder until it is sent.
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Tray {
    notes: Vec<Note>,
}

/// Where the tray of the worktree at `worktree` is kept (runs git).
fn tray_file(worktree: &Path) -> Result<std::path::PathBuf, String> {
    let out = crate::git::run(
        Some(worktree),
        &["rev-parse", "--git-path", "gitcraft-handoff.json"],
    )?;
    Ok(crate::remote::resolve(
        worktree,
        out.trim_end_matches(['\n', '\r']),
    ))
}

pub(crate) fn load(worktree: &Path) -> Result<Vec<Note>, String> {
    let file = tray_file(worktree)?;
    match crate::remote::read_to_string(&file) {
        Ok(text) if text.trim().is_empty() => Ok(Vec::new()),
        Ok(text) => serde_json::from_str::<Tray>(&text)
            .map(|t| t.notes)
            .map_err(|e| format!("the notes tray can't be read: {e}")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(format!("could not read the notes tray: {e}")),
    }
}

/// A number for a change of a tray, larger than every one before it in this process: saves
/// run on threads of their own, and one older than the last saved is dropped (see
/// [`save_in_order`]).
pub(crate) fn next_change() -> u64 {
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// Saves the tray as it was at change `seq` (from [`next_change`]): one that comes after a
/// later change's save is dropped, so the file always ends up with the latest.
pub(crate) fn save_in_order(worktree: &Path, notes: &[Note], seq: u64) -> Result<(), String> {
    use std::collections::HashMap;
    use std::sync::Mutex;
    static LAST: Mutex<Option<HashMap<std::path::PathBuf, u64>>> = Mutex::new(None);
    let mut last = LAST
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let map = last.get_or_insert_with(HashMap::new);
    if map.get(worktree).is_some_and(|done| *done > seq) {
        return Ok(());
    }
    map.insert(worktree.to_owned(), seq);
    save(worktree, notes)
}

pub(crate) fn save(worktree: &Path, notes: &[Note]) -> Result<(), String> {
    let file = tray_file(worktree)?;
    let text = serde_json::to_string_pretty(&Tray {
        notes: notes.to_vec(),
    })
    .map_err(|e| e.to_string())?;
    crate::remote::write(&file, text.as_bytes())
        .map_err(|e| format!("could not save the notes tray: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(path: &str, line: u64, text: &str) -> Note {
        Note {
            path: path.to_owned(),
            line,
            old: false,
            code: "let x = 1;".to_owned(),
            text: text.to_owned(),
            source: String::new(),
        }
    }

    #[test]
    fn notes_become_one_numbered_message() {
        let mut gone = note("src/ü b.rs", 4, "Why remove this?");
        gone.old = true;
        let msg = notes_message(&[
            note("src/a.rs", 12, "Make it a constant.\nAnd name it."),
            gone,
        ]);
        assert!(
            msg.contains(
                "\n1. src/a.rs:12\n   > let x = 1;\n   Make it a constant.\n   And name it.\n"
            ),
            "{msg}"
        );
        assert!(msg.contains("2. src/ü b.rs:4 (a removed line)"), "{msg}");
    }

    #[test]
    fn a_conflict_gives_all_three_sides_fenced_against_their_own_backticks() {
        let msg = conflict_message(
            "a.md",
            "main",
            "feature",
            &[("x ```\n", Some("b\n"), "y\n"), ("o", None, "t")],
        );
        assert!(msg.starts_with("a.md has 2 merge conflicts"), "{msg}");
        assert!(msg.contains("ours (main):\n````\nx ```\n````"), "{msg}");
        assert!(
            msg.contains("base (what both started from):\n```\nb\n```"),
            "{msg}"
        );
        assert_eq!(msg.matches("base (").count(), 1);
        assert!(msg.contains("theirs (feature):\n```\nt\n```"), "{msg}");
    }

    #[test]
    fn a_long_log_is_cut_to_its_end() {
        let log: String = (1..=500).map(|i| format!("line {i}\n")).collect();
        let msg = ci_message("CI / test", &log);
        assert!(
            msg.contains("(its last lines)")
                && msg.contains("line 500")
                && !msg.contains("line 300\n"),
            "{}",
            &msg[..200]
        );
        let short = ci_message("x", "boom\n");
        assert!(
            !short.contains("last lines") && short.contains("boom"),
            "{short}"
        );
        assert!(ci_message("x", "").contains("```"));
    }

    #[test]
    fn a_thread_keeps_its_place_and_its_voices() {
        let msg = thread_message(
            "src/a.rs",
            Some(7),
            &[
                ("ana".to_owned(), "Off by one?".to_owned()),
                ("bo".to_owned(), "Yes".to_owned()),
            ],
        );
        assert!(msg.starts_with("A review thread on src/a.rs:7."), "{msg}");
        assert!(
            msg.contains("\nana:\n  Off by one?\n\nbo:\n  Yes\n"),
            "{msg}"
        );
    }

    #[test]
    fn the_agent_terminal_is_found_by_its_name() {
        let agents = ["Claude Code".to_owned(), "Codex".to_owned()];
        let list = [
            ("zsh", false),
            ("Claude Code", false),
            ("Codex", true),
            ("zsh", false),
        ];
        assert_eq!(
            pick_terminal(&list, &agents, None),
            Some(1),
            "an ended agent is skipped"
        );
        assert_eq!(
            pick_terminal(&[("zsh", false), ("bash", false)], &agents, None),
            None,
            "never a shell by itself"
        );
        assert_eq!(
            pick_terminal(&list, &agents, Some((3, "zsh"))),
            Some(3),
            "one picked"
        );
        assert_eq!(
            pick_terminal(&list, &agents, Some((0, "Codex"))),
            Some(1),
            "picked, then moved"
        );
        assert_eq!(pick_terminal(&[], &agents, None), None);
    }

    #[test]
    fn the_tray_is_kept_per_worktree_and_reads_back() -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!("gitcraft-tray-{} é", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        crate::git::run(Some(&dir), &["init", "-q"])?;
        assert!(load(&dir)?.is_empty());
        let notes = vec![note("a.rs", 1, "quote \" and ünïcode"), note("b.rs", 2, "")];
        save(&dir, &notes)?;
        assert_eq!(load(&dir)?, notes);
        save(&dir, &[])?;
        assert!(load(&dir)?.is_empty());
        std::fs::write(dir.join(".git/gitcraft-handoff.json"), "{oops")
            .map_err(|e| e.to_string())?;
        let err = load(&dir).err().unwrap_or_default();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(err.contains("tray"), "{err}");
        Ok(())
    }
}
