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
fn log(
    repo: &Path,
    format: &str,
    notes: Option<&str>,
    excluded: &[String],
) -> Result<String, String> {
    let max = format!("--max-count={MOST}");
    let format = format!("--format={format}");
    let notes = notes.map(|n| format!("--notes={n}"));
    let mut args = vec![
        "-c",
        "i18n.logOutputEncoding=UTF-8",
        "log",
        "--no-show-signature",
        &max,
        &format,
    ];
    let revisions = crate::git::history_scope::revision_args(None, excluded);
    args.extend(revisions.iter().map(String::as_str));
    args.extend(notes.as_deref());
    if notes.is_none() {
        args.push("--no-notes");
    }
    crate::git::run(Some(repo), &args)
}

/// What every commit (the newest [`MOST`] of every branch) records of its authorship, by hash;
/// commits that record nothing left out.
#[cfg(test)]
pub fn all(repo: &Path) -> Result<HashMap<String, Attribution>, String> {
    all_scoped(
        repo,
        &crate::git::history_scope::default_hidden_namespaces(),
    )
}

pub fn all_scoped(
    repo: &Path,
    excluded: &[String],
) -> Result<HashMap<String, Attribution>, String> {
    let failed = |e: String| format!("Could not read who wrote the commits: {e}");
    let (git_ai, agent_trace) = notes_refs(repo).map_err(failed)?;
    // Fields ended by NULs, which no message or note can hold: each commit is so many of them.
    let main = log(
        repo,
        "%H%x00%an%x00%ae%x00%B%x00%N%x00",
        git_ai.then_some(GIT_AI),
        excluded,
    )
    .map_err(failed)?;
    let traces: HashMap<String, String> = if agent_trace {
        let out = log(repo, "%H%x00%N%x00", Some(AGENT_TRACE), excluded).map_err(failed)?;
        records(&out, 2)
            .filter(|f| !f[1].trim().is_empty())
            .map(|f| (f[0].to_owned(), f[1].to_owned()))
            .collect()
    } else {
        HashMap::new()
    };
    let mut out = HashMap::new();
    for f in records(&main, 5) {
        let note = Some(f[4]).filter(|n| !n.trim().is_empty());
        let trace = traces.get(f[0]).map(String::as_str);
        if let Some(a) = attribute(f[1], f[2], f[3], note, trace) {
            out.insert(f[0].to_owned(), a);
        }
    }
    Ok(out)
}

/// The records of `git log` output whose format ends each of its `n` fields with a NUL (and
/// git each record with a line end, read as the start of the next).
fn records(out: &str, n: usize) -> impl Iterator<Item = Vec<&str>> {
    let fields: Vec<&str> = out.split('\0').collect();
    let whole = fields.len() / n;
    (0..whole)
        .map(move |i| {
            let mut record: Vec<&str> = fields[i * n..i * n + n].to_vec();
            record[0] = record[0].trim_start_matches('\n');
            record
        })
        .filter(|r| !r[0].is_empty())
        .collect::<Vec<_>>()
        .into_iter()
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
            "--format=%an%x00%ae%x00%B%x00%N%x00",
            "--end-of-options",
            hash,
        ],
    )
    .map_err(|e| format!("Could not read who wrote {hash}: {e}"))?;
    let f: Vec<&str> = out.split('\0').collect();
    if f.len() < 4 {
        return Ok(None);
    }
    let field = |i: usize| f.get(i).copied().unwrap_or_default();
    let note = Some(field(3)).filter(|n| !n.trim().is_empty());
    // An Agent Trace note only where the repository has some (one more call otherwise saved).
    let trace = if field(3).trim().is_empty() && notes_refs(repo).is_ok_and(|(_, t)| t) {
        crate::git::run(Some(repo), &["notes", "--ref=agent-trace", "show", hash]).ok()
    } else {
        None
    };
    Ok(attribute(
        field(0),
        field(1),
        field(2),
        note,
        trace.as_deref(),
    ))
}
