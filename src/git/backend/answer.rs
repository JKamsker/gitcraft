//! Reads git answers from the repository's own files alone, answered in-process with the bytes
//! and the exit code git gives them: a lookup in the config, `rev-parse --verify`, `cat-file`,
//! `symbolic-ref`, `for-each-ref`, `ls-tree`, `merge-base --is-ancestor`, `notes`, `bisect
//! log`. A reload asks a hundred of these, each a git process of 50 to 300 ms on Windows; here
//! they take a millisecond or two (see `command::execute`, which asks first).
//!
//! Only the forms whose answer is certain are taken. Any other form or option, and any
//! repository whose history is rewritten (grafts, replaced objects, shallow), is `None`: git
//! is run, as before.

use std::path::Path;

use gix::bstr::{BString, ByteSlice};
mod status;

/// What git would have printed, and how it would have ended.
#[derive(Debug, PartialEq, Eq)]
pub(in crate::git) struct Answer {
    pub code: i32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

impl Answer {
    /// Construct a successful native answer with the provided stdout.
    fn out(stdout: impl Into<Vec<u8>>) -> Self {
        Self {
            code: 0,
            stdout: stdout.into(),
            stderr: Vec::new(),
        }
    }
    /// Construct a native answer with a failure exit code and stderr.
    fn fail(code: i32, stderr: &str) -> Self {
        Self {
            code,
            stdout: Vec::new(),
            stderr: stderr.as_bytes().to_vec(),
        }
    }
}

/// The answer git would give to `args` in `path`, made in-process; `None` when git is to run.
/// The caller has made sure the repository may be read natively (see `capability`).
pub(in crate::git) fn answer(path: &Path, args: &[&str]) -> Option<Answer> {
    // A read of local objects never fetches: the option changes nothing here.
    let args = args.strip_prefix(&["--no-lazy-fetch"]).unwrap_or(args);
    let (command, rest) = args.split_first()?;
    match *command {
        "config" => config::answer(path, rest),
        "rev-parse" => rev_parse(path, rest),
        "cat-file" => cat_file(path, rest),
        "symbolic-ref" => symbolic_ref(path, rest),
        "merge-base" => merge_base(path, rest),
        "notes" => notes(path, rest),
        "bisect" => bisect_log(path, rest),
        "ls-tree" => ls_tree(path, rest),
        "for-each-ref" => refs::answer(path, rest),
        "rev-list" => count(path, rest),
        "status" => status::answer(path, rest),
        _ => None,
    }
}

/// `rev-list --count [--exclude=<glob>…] --all`: the commits reachable from HEAD and every ref
/// but those excluded, in a repository without linked worktrees (whose HEADs `--all` counts
/// too).
fn count(path: &Path, args: &[&str]) -> Option<Answer> {
    let rest = args.strip_prefix(&["--count"])?;
    let (all, excludes) = rest.split_last()?;
    if *all != "--all" {
        return None;
    }
    let mut excluded = Vec::new();
    for exclude in excludes {
        let glob = exclude.strip_prefix("--exclude=")?;
        if glob.is_empty() || glob.contains(['?', '[', '\\']) {
            return None;
        }
        // Git matches these without minding `/`: `refs/gitcraft/*` takes `refs/gitcraft/a/b`.
        let pieces: Vec<String> = glob.split('*').map(regex::escape).collect();
        excluded.push(regex::Regex::new(&format!("^{}$", pieces.join(".*"))).ok()?);
    }
    let repo = open(path)?;
    plain(&repo)?;
    let worktrees = repo.common_dir().join("worktrees");
    if std::fs::read_dir(&worktrees).is_ok_and(|mut d| d.next().is_some()) {
        return None;
    }
    let mut tips = Vec::new();
    if let Some(head) = repo.head().ok()?.id() {
        tips.push(head.detach());
    }
    for reference in repo.references().ok()?.all().ok()? {
        let reference = reference.ok()?;
        let name = reference.name().as_bstr().to_str().ok()?;
        if excluded.iter().any(|e| e.is_match(name)) {
            continue;
        }
        // Refs of a tree or a blob add no commit.
        if let Some(id) = peel_to(
            &repo,
            target_id(&repo, &reference).ok()?,
            gix::object::Kind::Commit,
        )
        .ok()?
        {
            tips.push(id);
        }
    }
    tips.sort();
    tips.dedup();
    let mut commits = 0usize;
    if !tips.is_empty() {
        for info in repo.rev_walk(tips).all().ok()? {
            info.ok()?;
            commits += 1;
        }
    }
    Some(Answer::out(format!("{commits}\n")))
}

/// The name of the command answered, for the backend's metrics: `config`, `rev-parse`…
pub(in crate::git) fn name(args: &[&str]) -> &'static str {
    let args = args.strip_prefix(&["--no-lazy-fetch"]).unwrap_or(args);
    match args.first().copied() {
        Some("config") => "cmd:config",
        Some("rev-parse") => "cmd:rev-parse",
        Some("cat-file") => "cmd:cat-file",
        Some("symbolic-ref") => "cmd:symbolic-ref",
        Some("merge-base") => "cmd:merge-base",
        Some("notes") => "cmd:notes",
        Some("bisect") => "cmd:bisect",
        Some("ls-tree") => "cmd:ls-tree",
        Some("for-each-ref") => "cmd:for-each-ref",
        Some("rev-list") => "cmd:rev-list",
        Some("status") => "cmd:status",
        _ => "cmd:other",
    }
}

/// Open a repository for a single native command, returning `None` on failure.
fn open(path: &Path) -> Option<gix::Repository> {
    super::native::open(path).ok()
}

/// Raw object bytes for private comparisons. Resolve once, inspect the header before
/// allocating the blob, and never run filters, textconv, or a lazy fetch.
/// None retains Git for unsupported contexts; Err means the size limit was exceeded.
pub(super) fn bounded_blob(path: &Path, spec: &str, limit: usize) -> Option<Result<Vec<u8>, ()>> {
    let repo = open(path)?;
    plain(&repo)?;
    let id = resolve(&repo, spec).ok()??;
    let header = repo.try_find_header(id).ok()??;
    if header.kind() != gix::object::Kind::Blob {
        return None;
    }
    if header.size() > limit as u64 {
        return Some(Err(()));
    }
    let blob = repo.find_blob(id).ok()?;
    Some(if blob.data.len() <= limit {
        Ok(blob.detach().data)
    } else {
        Err(())
    })
}

/// `cat-file -s|-t <object>`, and `cat-file <type> <object>` of an object of that type. An
/// object not found is git's to report.
fn cat_file(path: &Path, args: &[&str]) -> Option<Answer> {
    let (flags, rest) = split(args, &["-s", "-t"])?;
    let (kind, spec) = match (flags.as_slice(), rest) {
        ([flag], [spec]) => (*flag, *spec),
        ([], [kind @ ("blob" | "commit" | "tree" | "tag"), spec]) => (*kind, *spec),
        _ => return None,
    };
    let repo = open(path)?;
    plain(&repo)?;
    let id = resolve(&repo, spec).ok()??;
    let object = repo.try_find_object(id).ok()??;
    Some(match kind {
        "-s" => Answer::out(format!("{}\n", object.data.len())),
        "-t" => Answer::out(format!("{}\n", object.kind)),
        kind if kind.as_bytes() == object.kind.as_bytes() => Answer::out(object.detach().data),
        _ => return None,
    })
}

/// `symbolic-ref --short -q HEAD` (or `-q --short`) of a branch, whose short name nothing else
/// could be read as.
fn symbolic_ref(path: &Path, args: &[&str]) -> Option<Answer> {
    let (flags, rest) = split(args, &["--short", "-q", "--quiet"])?;
    if rest != ["HEAD"] || !flags.contains(&"--short") || flags.len() != 2 {
        return None;
    }
    let repo = open(path)?;
    let head = repo.head().ok()?;
    let Some(name) = head.referent_name() else {
        return Some(Answer::fail(1, ""));
    };
    let full = name.as_bstr().to_str().ok()?;
    let short = full.strip_prefix("refs/heads/")?;
    // Git writes `heads/<name>` when `<name>` alone could be read as another ref.
    if repo.git_dir().join(short).exists() || repo.common_dir().join(short).exists() {
        return None;
    }
    for rule in ["refs/", "refs/tags/"] {
        if repo
            .try_find_reference(format!("{rule}{short}").as_str())
            .ok()?
            .is_some()
        {
            return None;
        }
    }
    Some(Answer::out(format!("{short}\n")))
}

/// `merge-base --is-ancestor <a> <b>` of two commits.
fn merge_base(path: &Path, args: &[&str]) -> Option<Answer> {
    let (flags, rest) = split(args, &["--is-ancestor"])?;
    let ([_], [a, b]) = (flags.as_slice(), rest) else {
        return None;
    };
    let repo = open(path)?;
    plain(&repo)?;
    let a = resolve(&repo, &format!("{a}^{{commit}}")).ok()??;
    let b = resolve(&repo, &format!("{b}^{{commit}}")).ok()??;
    let ancestor = a == b || repo.merge_base(a, b).ok()?.is_some_and(|base| base == a);
    Some(Answer {
        code: if ancestor { 0 } else { 1 },
        stdout: Vec::new(),
        stderr: Vec::new(),
    })
}

mod config;

mod refs;

#[cfg(test)]
#[path = "answer_tests.rs"]
mod tests;

mod metadata;
use metadata::bisect_log;
use metadata::ls_tree;
use metadata::notes;
#[cfg(test)]
use metadata::tree_order;

mod revision;
use revision::peel_to;
use revision::plain;
use revision::plain_path;
use revision::reference_id;
use revision::resolve;
use revision::rev_parse;
use revision::split;
use revision::target_id;
