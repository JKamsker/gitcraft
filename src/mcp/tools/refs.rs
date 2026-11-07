//! Reading branches, remotes, tags and stashes.

use serde_json::{Value, json};

use super::{Ctx, Kind, ToolDef, object, short};
use crate::git;
use crate::mcp::output::{capped, without_credentials};

pub(super) const TOOLS: &[&ToolDef] = &[&BRANCHES];

/// The most branches (of each remote, too), tags and stashes listed.
const MAX_REFS: usize = 500;
/// The most remotes listed.
const MAX_REMOTES: usize = 50;

static BRANCHES: ToolDef = ToolDef {
    name: "branches",
    title: "Branches and remotes",
    description: "Local branches with the commit each is at, its upstream and how far ahead and \
                  behind it is; the remotes with their URLs and branches; tags; stashes; \
                  worktrees.",
    kind: Kind::Read,
    schema: || object(json!({}), &[]),
    run: branches,
};

fn branches(ctx: &Ctx, a: &crate::mcp::args::Args) -> Result<Value, String> {
    a.only(&[])?;
    let refs = git::refs(&ctx.repo)?;
    let remotes = git::remotes(&ctx.repo)?;
    let stashes = git::stashes(&ctx.repo).unwrap_or_default();
    let worktrees = git::worktrees(&ctx.repo, None).unwrap_or_default();
    let mut out = json!({
        "current": refs.current,
        "head": refs.head,
    });
    let local: Vec<Value> = refs
        .local
        .iter()
        .map(|b| {
            let (ahead, behind) = b.ahead_behind();
            json!({
                "name": b.name,
                "commit": short(&b.target),
                "checked_out": b.is_head,
                "upstream": (!b.upstream.is_empty()).then_some(&b.upstream),
                "upstream_gone": b.upstream_gone(),
                "ahead": ahead,
                "behind": behind,
            })
        })
        .collect();
    put(&mut out, "local", local);
    let remotes: Vec<Value> = remotes
        .iter()
        .map(|r| {
            let names: Vec<&str> = refs
                .remotes
                .iter()
                .find(|(name, _)| *name == r.name)
                .map(|(_, bs)| bs.iter().map(|b| b.name.as_str()).collect())
                .unwrap_or_default();
            let (names, more) = capped(names, MAX_REFS);
            json!({
                "name": r.name,
                "url": without_credentials(&r.url),
                "branches": names,
                "branches_not_listed": more,
            })
        })
        .collect();
    let (remotes, more) = capped(remotes, MAX_REMOTES);
    out["remotes"] = json!(remotes);
    if more > 0 {
        out["remotes_not_listed"] = json!(more);
    }
    let tags: Vec<Value> = refs
        .tags
        .iter()
        .map(|t| json!({ "name": t.name, "commit": short(&t.target) }))
        .collect();
    put(&mut out, "tags", tags);
    let stashes: Vec<Value> = stashes
        .iter()
        .map(|s| json!({ "name": s.name, "message": s.message, "date": s.date }))
        .collect();
    put(&mut out, "stashes", stashes);
    let worktrees: Vec<Value> = worktrees
        .iter()
        .map(|w| {
            json!({
                "path": w.path.display().to_string(),
                "branch": w.branch,
                "commit": short(&w.head),
            })
        })
        .collect();
    put(&mut out, "worktrees", worktrees);
    Ok(out)
}

/// Puts at most [`MAX_REFS`] of `items` under `key`, and how many were left out.
fn put(out: &mut Value, key: &str, items: Vec<Value>) {
    let (items, more) = capped(items, MAX_REFS);
    out[key] = json!(items);
    if more > 0 {
        out[format!("{key}_not_listed")] = json!(more);
    }
}
