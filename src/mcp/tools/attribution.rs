//! Explicit recorded provenance, shared with the window's authorship lens.
use super::{Ctx, Kind, ToolDef, object, string};
use crate::mcp::args::Args;
use serde_json::{Value, json};

pub(super) const TOOLS: &[&ToolDef] = &[&ATTRIBUTION];
static ATTRIBUTION: ToolDef = ToolDef {
    name: "attribution",
    title: "Recorded human and agent attribution",
    description: "Read git-ai notes, Agent Trace and explicit agent author/co-author records for one immutable commit. Reports unknown coverage without guessing. No transcripts or credentials are returned.",
    kind: Kind::Read,
    schema: || {
        object(
            json!({"commit": string("Commit or revision; default HEAD")}),
            &[],
        )
    },
    run: read,
};
fn who(w: crate::authorship::Who) -> &'static str {
    use crate::authorship::Who;
    match w {
        Who::Agent => "agent",
        Who::Human => "human",
        Who::Mixed => "mixed",
        Who::Unknown => "unknown",
    }
}
fn read(ctx: &Ctx, args: &Args) -> Result<Value, String> {
    args.only(&["commit"])?;
    let commit = super::history::resolve_commit(ctx, args.opt_str("commit")?.unwrap_or("HEAD"))?;
    let Some(a) = crate::authorship::load::one(&ctx.repo, &commit)? else {
        return Ok(
            json!({"commit": commit, "who": "unknown", "source": null, "lines": [], "coverage": "No explicit attribution recorded"}),
        );
    };
    let mut files: Vec<_> = a.lines.iter().collect();
    files.sort_by_key(|(path, _)| *path);
    let mut remaining = 2000usize;
    let lines: Vec<_> = files
        .into_iter()
        .flat_map(|(path, ranges)| {
            ranges
                .iter()
                .map(move |(from, to, w)| json!({"path":path, "from":from, "to":to, "who":who(*w)}))
        })
        .take(remaining)
        .collect();
    let total: usize = a.lines.values().map(Vec::len).sum();
    remaining = total.saturating_sub(lines.len());
    Ok(
        json!({"commit":commit, "who":who(a.who), "agents":a.agents, "source":a.source.label(), "lines":lines, "ranges_not_listed":remaining, "coverage": if a.lines.is_empty() { "Commit-level declaration only; no line coverage" } else { "Only recorded ranges are attributed; other lines are unknown" }}),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mcp_reads_recorded_notes_and_keeps_uncovered_commits_unknown() -> Result<(), String> {
        let r = crate::git::fixture::Repo::new("mcp-attribution")?;
        let oid = r.commit("human", &[])?;
        let ctx = Ctx {
            repo: r.path().into(),
            agent: "test".into(),
            link_dir: None,
            wait: std::time::Duration::from_secs(1),
            cancelled: std::sync::Arc::new(|| false),
        };
        let result = super::super::call(&ATTRIBUTION, &ctx, &serde_json::Map::new())?;
        assert_eq!(result["who"], "unknown");
        r.git(&[
            "commit",
            "--allow-empty",
            "-m",
            "Agent helped\n\nCo-authored-by: Claude <noreply@anthropic.com>",
        ])?;
        let result = super::super::call(&ATTRIBUTION, &ctx, &serde_json::Map::new())?;
        assert_eq!(result["who"], "mixed");
        assert!(
            result["source"]
                .as_str()
                .ok_or("expected Some")?
                .contains("trailer")
        );
        let args = json!({"commit":oid});
        assert_eq!(
            super::super::call(&ATTRIBUTION, &ctx, args.as_object().ok_or("expected Some")?)?["who"],
            "unknown"
        );
        Ok(())
    }
}
