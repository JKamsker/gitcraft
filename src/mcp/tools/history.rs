//! Reading history: the graph's commits with their branches and tags, one commit in full, who
//! last changed each line of a file.

use serde_json::{Value, json};

use super::{Ctx, Kind, ToolDef, number, object, short, string};
use crate::git;
use crate::mcp::args::{self, Args};
use crate::mcp::output::{capped, clip};

pub(super) const TOOLS: &[&ToolDef] = &[&LOG, &COMMIT, &BLAME];

const MAX_LOG: u64 = 200;
/// History deeper than this isn't paged into (as in the window).
const MAX_SKIP: u64 = 100_000;

static LOG: ToolDef = ToolDef {
    name: "log",
    title: "Commit graph",
    description: "Commits newest first, as gitcraft's graph shows them: hash, parents (two or more \
                  for a merge), author, date, summary and the branches and tags on each. Every \
                  branch by default, or the history of `rev`; with `path`, only the commits that \
                  changed that file (following renames). Paged: pass `next_skip` back as `skip`.",
    kind: Kind::Read,
    schema: || {
        object(
            json!({
                "rev": string("Only the history of this revision (a branch, tag or commit). Default: every branch."),
                "path": string("Only commits that changed this file (relative to the top of the repository)."),
                "limit": number("How many commits.", 30, 1, MAX_LOG),
                "skip": number("How many of the newest to skip (paging).", 0, 0, MAX_SKIP),
            }),
            &[],
        )
    },
    run: log,
};

fn log(ctx: &Ctx, a: &Args) -> Result<Value, String> {
    a.only(&["rev", "path", "limit", "skip"])?;
    let rev = a.opt_str("rev")?.map(args::revision).transpose()?;
    let path = a.opt_str("path")?.map(args::repo_path).transpose()?;
    let limit = a.uint("limit", 30, 1, MAX_LOG)? as usize;
    let skip = a.uint("skip", 0, 0, MAX_SKIP)? as usize;
    if let Some(path) = path {
        return file_log(ctx, rev, path, skip, limit);
    }
    let from = rev.map(|r| vec![r.to_owned()]);
    // One more than asked: whether there is a next page.
    let mut commits = git::log(&ctx.repo, from.as_deref(), skip, limit + 1)?;
    let more = commits.len() > limit;
    commits.truncate(limit);
    let mut page = json!({
        "commits": commits.iter().map(commit_row).collect::<Vec<_>>(),
    });
    if more {
        page["next_skip"] = json!(skip + limit);
    }
    Ok(page)
}

fn commit_row(c: &git::Commit) -> Value {
    json!({
        "hash": c.hash,
        "short": short(&c.hash),
        "parents": c.parents,
        "author": c.author,
        "email": c.email,
        "date": c.date,
        "summary": clip(&c.summary, MAX_SUMMARY),
        "refs": capped(c.refs.clone(), MAX_REFS_SHOWN).0,
        "refs_not_listed": c.refs.len().saturating_sub(MAX_REFS_SHOWN),
    })
}

/// The longest summary, and the most branches and tags of one commit, a list shows.
const MAX_SUMMARY: usize = 300;
const MAX_REFS_SHOWN: usize = 30;
/// The longest message, and line of a file, shown whole.
const MAX_MESSAGE: usize = 20_000;
const MAX_LINE_TEXT: usize = 500;

fn file_log(
    ctx: &Ctx,
    rev: Option<&str>,
    path: &str,
    skip: usize,
    limit: usize,
) -> Result<Value, String> {
    let all = git::file_history(&ctx.repo, rev, path)?;
    let total = all.len();
    let commits: Vec<Value> = all
        .into_iter()
        .skip(skip)
        .take(limit)
        .map(|c| {
            json!({
                "hash": c.hash,
                "short": short(&c.hash),
                "author": c.author,
                "email": c.email,
                "time": c.time,
                "summary": clip(&c.summary, MAX_SUMMARY),
                "path": c.path,
            })
        })
        .collect();
    let mut page = json!({ "path": path, "commits": commits });
    if skip + limit < total {
        page["next_skip"] = json!(skip + limit);
    }
    Ok(page)
}

/// The most files of a commit listed.
const MAX_FILES: usize = 500;

static COMMIT: ToolDef = ToolDef {
    name: "show_commit",
    title: "One commit",
    description: "One commit in full: its whole message, author and committer with dates, \
                  parents, whether HEAD contains it, and the files it changed with lines added \
                  and removed. Use `diff` with `commit` for the changes themselves.",
    kind: Kind::Read,
    schema: || {
        object(
            json!({ "commit": string("The commit: a hash, branch, tag or HEAD~n. Default: HEAD.") }),
            &[],
        )
    },
    run: commit,
};

fn commit(ctx: &Ctx, a: &Args) -> Result<Value, String> {
    a.only(&["commit"])?;
    let rev = args::revision(a.opt_str("commit")?.unwrap_or("HEAD"))?;
    let hash = resolve_commit(ctx, rev)?;
    let d = git::commit_detail(&ctx.repo, &hash)?;
    let (files, more) = capped(d.files, MAX_FILES);
    let mut out = json!({
        "hash": d.hash,
        "parents": d.parents,
        "author": d.author,
        "email": d.email,
        "author_date": d.author_date,
        "committer": d.committer,
        "commit_date": d.commit_date,
        "message": clip(&d.message, MAX_MESSAGE),
        "in_head": d.in_head,
        "files": files.iter().map(|f| json!({
            "status": f.status.to_string(),
            "path": f.path,
            "old_path": f.old_path,
            "added": f.added,
            "removed": f.removed,
        })).collect::<Vec<_>>(),
    });
    if more > 0 {
        out["files_not_listed"] = json!(more);
    }
    Ok(out)
}

/// The full hash of the commit `rev` names, or why it names none.
pub(super) fn resolve_commit(ctx: &Ctx, rev: &str) -> Result<String, String> {
    let rev = args::revision(rev)?;
    git::run(
        Some(&ctx.repo),
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{rev}^{{commit}}"),
        ],
    )
    .map(|out| out.trim().to_owned())
    .ok()
    .filter(|h| !h.is_empty())
    .ok_or_else(|| format!("`{rev}` is not a commit in this repository"))
}

/// Refuses a path git doesn't find in the working tree as a file of the repository: one gone,
/// or behind a symbolic link to a folder (which `blame` would follow, out of the repository).
/// git looks at the path itself, here or on the repository's machine.
fn in_working_tree(ctx: &Ctx, path: &str) -> Result<(), String> {
    let gone = git::run(
        Some(&ctx.repo),
        &[
            "--literal-pathspecs",
            "diff-files",
            "--name-only",
            "--diff-filter=D",
            "-z",
            "--",
            path,
        ],
    )?;
    if gone.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "`{path}` isn't in the working tree as a file of the repository (deleted, or behind \
             a symbolic link): pass `rev` to blame it as committed"
        ))
    }
}

/// The most lines one blame call returns.
const MAX_BLAME: u64 = 400;

static BLAME: ToolDef = ToolDef {
    name: "blame",
    title: "Blame",
    description: "Who last changed each line of a file, and in which commit, for lines \
                  `start_line` to `end_line` (at most 400 at a time). As the file is in the \
                  working tree, or at `rev`.",
    kind: Kind::Read,
    schema: || {
        object(
            json!({
                "path": string("The file, relative to the top of the repository."),
                "start_line": number("First line (1 is the first).", 1, 1, u32::MAX.into()),
                "end_line": { "type": "integer", "minimum": 1, "description": "Last line; default start_line + 199." },
                "rev": string("Blame the file as it is at this revision. Default: the working tree."),
            }),
            &["path"],
        )
    },
    run: blame,
};

fn blame(ctx: &Ctx, a: &Args) -> Result<Value, String> {
    a.only(&["path", "start_line", "end_line", "rev"])?;
    let path = args::repo_path(a.str("path")?)?;
    let rev = a.opt_str("rev")?.map(args::revision).transpose()?;
    let start = a.uint("start_line", 1, 1, u32::MAX.into())?;
    let end = a.uint(
        "end_line",
        start + MAX_BLAME / 2 - 1,
        start,
        u32::MAX.into(),
    )?;
    if end - start + 1 > MAX_BLAME {
        return Err(format!(
            "at most {MAX_BLAME} lines at a time: end_line can be {} at most",
            start + MAX_BLAME - 1
        ));
    }
    if rev.is_none() {
        in_working_tree(ctx, path)?;
    }
    let lines = git::blame_plain(&ctx.repo, rev, path, (start, end))?;
    let more = lines.len() as u64 == end - start + 1;
    let rows: Vec<Value> = lines
        .iter()
        .enumerate()
        .map(|(i, l)| {
            json!({
                "line": start as usize + i,
                "commit": if l.uncommitted() { "uncommitted".to_owned() } else { short(&l.hash).to_owned() },
                "author": l.author,
                "time": l.time,
                "summary": clip(&l.summary, MAX_SUMMARY),
                "text": clip(&l.text, MAX_LINE_TEXT),
            })
        })
        .collect();
    let mut page = json!({ "path": path, "lines": rows });
    if more {
        page["next_start_line"] = json!(end + 1);
    }
    Ok(page)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    /// A file committed in `docs/`, then `docs` swapped for a link to a folder outside.
    #[test]
    fn blame_never_follows_a_link_out_of_the_repository() {
        let base = std::env::temp_dir().join(format!("gitcraft-blame-link-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let (dir, outside) = (base.join("repo"), base.join("secret"));
        let _ = std::fs::create_dir_all(dir.join("docs"));
        let _ = std::fs::create_dir_all(&outside);
        let _ = std::fs::write(dir.join("docs/key"), "committed\n");
        let _ = std::fs::write(outside.join("key"), "SECRET\n");
        for args in [
            &["init", "-q"][..],
            &["-c", "user.name=t", "-c", "user.email=t@t", "add", "-A"],
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "commit",
                "-q",
                "-m",
                "docs",
            ],
        ] {
            assert!(git::run(Some(&dir), args).is_ok());
        }
        let _ = std::fs::remove_dir_all(dir.join("docs"));
        let _ = std::os::unix::fs::symlink(&outside, dir.join("docs"));
        let ctx = Ctx {
            repo: dir.clone(),
            agent: "t".into(),
            link_dir: None,
            wait: std::time::Duration::from_secs(1),
            cancelled: std::sync::Arc::new(|| false),
        };
        let at = |rev: Option<&str>| {
            let mut args = serde_json::Map::new();
            args.insert("path".into(), json!("docs/key"));
            if let Some(rev) = rev {
                args.insert("rev".into(), json!(rev));
            }
            blame(&ctx, &Args::new(&args))
        };
        let worktree = at(None);
        let committed = at(Some("HEAD"));
        let _ = std::fs::remove_dir_all(&base);
        assert!(worktree.is_err_and(|e| e.contains("symbolic link")));
        let text = committed.map(|v| v.to_string()).unwrap_or_default();
        assert!(
            text.contains("committed") && !text.contains("SECRET"),
            "{text}"
        );
    }
}
