//! Reading the working tree: what changed, the changes themselves, conflicts left to resolve.

use serde_json::{Value, json};

use super::{Ctx, Kind, ToolDef, boolean, number, object, short, string};
use crate::git::{self, Area, StatusFile};
use crate::mcp::args::{self, Args};
use crate::mcp::output::{capped, text_page};

pub(super) const TOOLS: &[&ToolDef] = &[&STATUS, &DIFF, &CONFLICTS];

/// The most files of one kind `status` lists.
const MAX_FILES: usize = 300;

static STATUS: ToolDef = ToolDef {
    name: "status",
    title: "Working tree status",
    description: "The checked-out branch (or a detached HEAD), its upstream and how far ahead \
                  and behind it is, the files staged, changed but not staged, untracked and \
                  conflicted, and any merge, rebase, cherry-pick or revert in progress.",
    kind: Kind::Read,
    schema: || object(json!({}), &[]),
    run: status,
};

fn status(ctx: &Ctx, a: &Args) -> Result<Value, String> {
    a.only(&[])?;
    let work = git::status(&ctx.repo)?;
    let refs = git::refs(&ctx.repo)?;
    let current = refs
        .current
        .as_ref()
        .and_then(|c| refs.local.iter().find(|b| &b.name == c));
    let branch = current.map(|b| {
        let (ahead, behind) = b.ahead_behind();
        json!({
            "name": b.name,
            "upstream": (!b.upstream.is_empty()).then_some(&b.upstream),
            "upstream_gone": b.upstream_gone(),
            "ahead": ahead,
            "behind": behind,
        })
    });
    let (untracked, unstaged): (Vec<StatusFile>, Vec<StatusFile>) = work
        .unstaged
        .into_iter()
        .partition(|f| f.area == Area::Untracked);
    let mut out = json!({
        "head": refs.head,
        "branch": branch,
        "detached": refs.current.is_none() && !refs.head.is_empty(),
        "in_progress": work.in_progress.map(|s| s.label()),
        "clean": work.staged.is_empty() && unstaged.is_empty() && untracked.is_empty()
            && work.conflicted.is_empty(),
    });
    for (key, files) in [
        ("staged", work.staged),
        ("unstaged", unstaged),
        ("untracked", untracked),
        ("conflicted", work.conflicted),
    ] {
        let (files, more) = capped(files, MAX_FILES);
        out[key] = json!(files.iter().map(file_row).collect::<Vec<_>>());
        if more > 0 {
            out[format!("{key}_not_listed")] = json!(more);
        }
    }
    Ok(out)
}

fn file_row(f: &StatusFile) -> Value {
    let status = match f.code {
        'M' => "modified",
        'A' => "added",
        'D' => "deleted",
        'R' => "renamed",
        'C' => "copied",
        'T' => "type changed",
        '?' => "untracked",
        'U' => "conflicted",
        _ => "changed",
    };
    match &f.old_path {
        Some(old) => json!({ "path": f.path, "status": status, "old_path": old }),
        None => json!({ "path": f.path, "status": status }),
    }
}

/// The default and largest page of a diff, in bytes.
const DIFF_PAGE: u64 = 40_000;
const MAX_DIFF_PAGE: u64 = 200_000;

static DIFF: ToolDef = ToolDef {
    name: "diff",
    title: "Diff",
    description: "The changes as a unified diff: of the working tree against the index \
                  (`staged: false`, the default) or of the index against HEAD (`staged: true`); \
                  or with `commit`, what that commit changed (a merge: against its first \
                  parent). `path` limits it to one file or folder. Untracked files aren't in \
                  it (see `status`). Paged by bytes: pass `next_offset` back as `offset`.",
    kind: Kind::Read,
    schema: || {
        object(
            json!({
                "commit": string("Show what this commit changed instead of the working tree."),
                "staged": boolean("Working tree only: the staged changes rather than the unstaged ones.", false),
                "path": string("Only this file or folder, relative to the top of the repository."),
                "context": number("Lines of context around each change.", 3, 0, 100),
                "offset": number("Byte offset to start the page at.", 0, 0, u64::from(u32::MAX)),
                "max_bytes": number("Size of the page.", DIFF_PAGE, 1000, MAX_DIFF_PAGE),
            }),
            &[],
        )
    },
    run: diff,
};

fn diff(ctx: &Ctx, a: &Args) -> Result<Value, String> {
    a.only(&["commit", "staged", "path", "context", "offset", "max_bytes"])?;
    let commit = a.opt_str("commit")?.map(args::revision).transpose()?;
    let staged = a.bool("staged", false)?;
    let path = a.opt_str("path")?.map(args::repo_path).transpose()?;
    let context = format!("-U{}", a.uint("context", 3, 0, 100)?);
    let offset = a.uint("offset", 0, 0, u64::from(u32::MAX))? as usize;
    let max = a.uint("max_bytes", DIFF_PAGE, 1000, MAX_DIFF_PAGE)? as usize;
    if commit.is_some() && staged {
        return Err("`staged` is for the working tree: leave it out with `commit`".to_owned());
    }
    let hash = commit
        .map(|c| super::history::resolve_commit(ctx, c))
        .transpose()?;
    let text = git::run(
        Some(&ctx.repo),
        &diff_args(hash.as_deref(), staged, path, &context),
    )?;
    let mut page = text_page(&text, offset, max);
    page["of"] = json!(match &hash {
        Some(h) => format!("commit {}", short(h)),
        None if staged => "staged changes".to_owned(),
        None => "unstaged changes".to_owned(),
    });
    Ok(page)
}

/// The git command for [`diff`]: no external diff programs or text conversions (an agent's
/// request never runs a program the repository configures), paths taken literally.
fn diff_args<'a>(
    commit: Option<&'a str>,
    staged: bool,
    path: Option<&'a str>,
    context: &'a str,
) -> Vec<&'a str> {
    let mut args = vec!["--literal-pathspecs"];
    match commit {
        Some(_) => args.extend([
            "show",
            "--no-show-signature",
            "--format=",
            "--diff-merges=first-parent",
            "--root",
        ]),
        None => {
            args.push("diff");
            if staged {
                args.push("--cached");
            }
        }
    }
    args.extend([
        "--no-color",
        "--no-ext-diff",
        "--no-textconv",
        "-M",
        context,
    ]);
    // The revision last of all: after `--end-of-options` nothing is an option any more.
    if let Some(hash) = commit {
        args.extend(["--end-of-options", hash]);
    }
    if let Some(path) = path {
        args.extend(["--", path]);
    }
    args
}

static CONFLICTS: ToolDef = ToolDef {
    name: "conflicts",
    title: "Conflicts",
    description: "The merge, rebase, cherry-pick or revert in progress and the files left \
                  conflicted. With `path`, that file's conflicts as git shows them: the combined \
                  diff with both sides of each conflict (paged like `diff`).",
    kind: Kind::Read,
    schema: || {
        object(
            json!({
                "path": string("A conflicted file, relative to the top of the repository."),
                "offset": number("Byte offset to start the page at.", 0, 0, u64::from(u32::MAX)),
                "max_bytes": number("Size of the page.", DIFF_PAGE, 1000, MAX_DIFF_PAGE),
            }),
            &[],
        )
    },
    run: conflicts,
};

fn conflicts(ctx: &Ctx, a: &Args) -> Result<Value, String> {
    a.only(&["path", "offset", "max_bytes"])?;
    let path = a.opt_str("path")?.map(args::repo_path).transpose()?;
    let offset = a.uint("offset", 0, 0, u64::from(u32::MAX))? as usize;
    let max = a.uint("max_bytes", DIFF_PAGE, 1000, MAX_DIFF_PAGE)? as usize;
    let work = git::status(&ctx.repo)?;
    let files: Vec<&str> = work.conflicted.iter().map(|f| f.path.as_str()).collect();
    let mut out = json!({
        "in_progress": work.in_progress.map(|s| s.label()),
        "conflicted": files,
    });
    if let Some(path) = path {
        if !files.contains(&path) {
            return Err(format!("`{path}` has no conflicts"));
        }
        let shown = git::run(
            Some(&ctx.repo),
            &[
                "--literal-pathspecs",
                "diff",
                "--no-color",
                "--no-ext-diff",
                "--no-textconv",
                "--",
                path,
            ],
        )?;
        out["path"] = json!(path);
        out["diff"] = text_page(&shown, offset, max);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_diff_never_runs_the_repositorys_programs() {
        let args = diff_args(Some("abc"), false, Some("a.txt"), "-U3");
        for flag in ["--no-ext-diff", "--no-textconv", "--literal-pathspecs"] {
            assert!(args.contains(&flag));
        }
        // The commit after `--end-of-options`, the path after `--`.
        let at = |w: &str| args.iter().position(|a| *a == w);
        assert!(at("--end-of-options") < at("abc"));
        assert_eq!(args.last(), Some(&"a.txt"));
        assert!(at("--") < at("a.txt"));
        let staged = diff_args(None, true, None, "-U0");
        assert_eq!(staged[..3], ["--literal-pathspecs", "diff", "--cached"]);
    }

    #[test]
    fn statuses_read_as_words() {
        let file = |code| StatusFile {
            path: "p".into(),
            old_path: None,
            code,
            area: Area::Unstaged,
        };
        assert_eq!(file_row(&file('M'))["status"], "modified");
        assert_eq!(file_row(&file('?'))["status"], "untracked");
        let renamed = StatusFile {
            old_path: Some("o".into()),
            ..file('R')
        };
        assert_eq!(file_row(&renamed)["old_path"], "o");
    }
}
