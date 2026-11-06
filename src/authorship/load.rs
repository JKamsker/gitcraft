//! Reading what commits record of their authorship, through the git command layer (a
//! repository in WSL or over SSH is read there).

use std::collections::HashMap;
use std::path::Path;

use super::{Attribution, attribute};

/// Most commits read at a time (of every branch, newest first).
pub const MOST: usize = 20_000;

const GIT_AI: &str = "refs/notes/ai";
const AGENT_TRACE: &str = "refs/notes/agent-trace";

/// Which of the two notes refs the repository has.
fn notes_refs(repo: &Path) -> Result<(bool, bool), String> {
    let out = crate::git::run(
        Some(repo),
        &["for-each-ref", "--format=%(refname)", GIT_AI, AGENT_TRACE],
    )?;
    Ok((
        out.lines().any(|l| l == GIT_AI),
        out.lines().any(|l| l == AGENT_TRACE),
    ))
}

/// `git log` of every branch with `format`, `notes` the notes ref `%N` reads (none: `None`).
fn log(repo: &Path, format: &str, notes: Option<&str>) -> Result<String, String> {
    let max = format!("--max-count={MOST}");
    let format = format!("--format={format}");
    let notes = notes.map(|n| format!("--notes={n}"));
    let mut args = vec![
        "-c",
        "i18n.logOutputEncoding=UTF-8",
        "log",
        "--no-show-signature",
        // The notes' own history isn't the repository's.
        "--exclude=refs/notes/*",
        "--all",
        &max,
        &format,
    ];
    args.extend(notes.as_deref());
    if notes.is_none() {
        args.push("--no-notes");
    }
    crate::git::run(Some(repo), &args)
}

/// What every commit (the newest [`MOST`] of every branch) records of its authorship, by hash;
/// commits that record nothing left out.
pub fn all(repo: &Path) -> Result<HashMap<String, Attribution>, String> {
    let failed = |e: String| format!("Could not read who wrote the commits: {e}");
    let (git_ai, agent_trace) = notes_refs(repo).map_err(failed)?;
    let main = log(
        repo,
        "%H%x1f%an%x1f%ae%x1f%B%x1f%N%x1e",
        git_ai.then_some(GIT_AI),
    )
    .map_err(failed)?;
    let traces: HashMap<String, String> = if agent_trace {
        log(repo, "%H%x1f%N%x1e", Some(AGENT_TRACE))
            .map_err(failed)?
            .split('\x1e')
            .filter_map(|r| {
                let (hash, note) = r.trim_start_matches('\n').split_once('\x1f')?;
                (!note.trim().is_empty()).then(|| (hash.to_owned(), note.to_owned()))
            })
            .collect()
    } else {
        HashMap::new()
    };
    let mut out = HashMap::new();
    for record in main.split('\x1e') {
        let mut fields = record.trim_start_matches('\n').splitn(5, '\x1f');
        let (Some(hash), Some(author), Some(email), Some(message)) =
            (fields.next(), fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        let note = fields.next().filter(|n| !n.trim().is_empty());
        let trace = traces.get(hash).map(String::as_str);
        if let Some(a) = attribute(author, email, message, note, trace) {
            out.insert(hash.to_owned(), a);
        }
    }
    Ok(out)
}

/// What one commit records of its authorship.
pub fn one(repo: &Path, hash: &str) -> Result<Option<Attribution>, String> {
    if hash.starts_with('-') {
        return Err(format!("{hash} is not a commit"));
    }
    let out = crate::git::run(
        Some(repo),
        &[
            "-c",
            "i18n.logOutputEncoding=UTF-8",
            "log",
            "-1",
            "--no-show-signature",
            "--notes=ai",
            "--format=%an%x1f%ae%x1f%B%x1f%N",
            "--end-of-options",
            hash,
        ],
    )
    .map_err(|e| format!("Could not read who wrote {hash}: {e}"))?;
    let mut fields = out.splitn(4, '\x1f');
    let (author, email, message) = (
        fields.next().unwrap_or_default(),
        fields.next().unwrap_or_default(),
        fields.next().unwrap_or_default(),
    );
    let note = fields.next().filter(|n| !n.trim().is_empty());
    // No note there: git says so and fails.
    let trace = crate::git::run(Some(repo), &["notes", "--ref=agent-trace", "show", hash]).ok();
    Ok(attribute(author, email, message, note, trace.as_deref()))
}
