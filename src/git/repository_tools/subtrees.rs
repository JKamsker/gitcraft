//! Explicit subtree imports, retained candidates and exact-commit publication.
//! Helpers and network operations run only after an app action, never while reading this view.
use super::{guard::Runner, remote_config::validate_url};
use crate::{
    git,
    git::drafts::{Context, deferred},
    push_preflight as preflight,
    settings::profiles::UserProfile,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
const REF: &str = "refs/gitcraft/subtrees/state";
const LIMIT: usize = 2 << 20;
const HISTORY: &str = "backend-subtree-history-explanation";
pub(crate) fn history_explanation() -> &'static str {
    crate::i18n::text(HISTORY)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum History {
    Squash,
    Full,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Source {
    pub id: String,
    pub prefix: String,
    pub remote: String,
    pub branch: String,
    pub history: History,
    pub fetched: Option<String>,
    pub imported: Option<String>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Kind {
    Associate,
    Update,
    Split,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Stage {
    Preparing,
    Running,
    Conflict,
    Ready,
    Accepting,
    Accepted,
    SplitReady,
    Sending,
    Sent,
    Paused,
    Closed,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Candidate {
    pub id: String,
    pub source: String,
    pub branch: String,
    pub old: String,
    pub upstream: Option<String>,
    pub kind: Kind,
    #[serde(default)]
    pub sign_commits: bool,
    pub stage: Stage,
    pub worktree: PathBuf,
    pub identity: Option<String>,
    pub result: Option<String>,
    pub notice: String,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct Registry {
    pub sources: Vec<Source>,
    pub candidates: Vec<Candidate>,
}
#[derive(Clone)]
pub(crate) struct Local {
    pub source: String,
    pub baseline: Option<String>,
    pub committed: String,
    pub staged: String,
    pub working: String,
    pub untracked: Vec<String>,
}
#[derive(Clone)]
pub(crate) struct View {
    pub state: Registry,
    pub version: String,
    pub branch: String,
    pub head: String,
    pub remotes: Vec<String>,
    pub sign_commits: bool,
    pub local: Option<Local>,
    pub candidate_diff: Option<(String, String)>,
    pub common: PathBuf,
    pub helper: String,
    route: String,
}
#[derive(Clone)]
pub(crate) struct Prepared {
    pub view: View,
    pub id: String,
    pub plan: preflight::Plan,
    pub context: crate::connections::Context,
    pub principal: Option<(String, String, String)>,
    pub generation: u64,
    pub renewed: bool,
}
fn oid(s: &str) -> bool {
    matches!(s.len(), 40 | 64) && s.bytes().all(|b| b.is_ascii_hexdigit())
}
fn id(s: &str) -> bool {
    !s.is_empty() && s.len() < 100 && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}
fn plain(s: &str) -> bool {
    !s.is_empty() && !s.starts_with('-') && s.trim() == s && !s.chars().any(char::is_control)
}
fn prefix(s: &str) -> bool {
    plain(s)
        && s.len() <= 1024
        && !s.contains('\\')
        && s.split('/')
            .all(|p| !p.is_empty() && p != "." && p != ".." && !p.eq_ignore_ascii_case(".git"))
}
fn branch_name(s: &str) -> bool {
    plain(s)
        && !s.contains([' ', ':', '^', '~', '?', '*', '[', '\\'])
        && !s.contains("..")
        && !s.contains("@{")
        && !s.ends_with(['.', '/'])
        && s.split('/')
            .all(|c| !c.is_empty() && !c.starts_with('.') && !c.ends_with(".lock"))
}
fn overlap(a: &str, b: &str) -> bool {
    a == b || a.starts_with(&format!("{b}/")) || b.starts_with(&format!("{a}/"))
}

fn resolve(ctx: &Context, rev: &str) -> Result<String, String> {
    let out = text(
        ctx,
        &[
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{rev}^{{commit}}"),
        ],
        &[0],
    )?;
    if !oid(&out) {
        return Err(crate::i18n::text("backend-invalid-immutable-subtree-commit-d12622").into());
    }
    Ok(out)
}
fn branch(ctx: &Context) -> Result<String, String> {
    text(ctx, &["symbolic-ref", "-q", "HEAD"], &[0, 1])
}
fn tree(ctx: &Context, head: &str, path: &str) -> Result<Option<String>, String> {
    let raw = bytes(
        ctx,
        &["--literal-pathspecs", "ls-tree", "-z", head, "--", path],
        &[0],
    )?;
    if raw.is_empty() {
        return Ok(None);
    }
    let s = std::str::from_utf8(&raw)
        .map_err(|_| crate::i18n::text("backend-invalid-vendored-tree-metadata-20f7eb"))?;
    let (meta, name) = s
        .trim_end_matches('\0')
        .split_once('\t')
        .ok_or(crate::i18n::text("backend-invalid-vendored-tree-e66d63"))?;
    let parts = meta.split(' ').collect::<Vec<_>>();
    if parts.len() != 3
        || parts[0] != "040000"
        || parts[1] != "tree"
        || name != path
        || !oid(parts[2])
    {
        return Err(crate::i18n::text(
            "backend-the-registered-prefix-must-be-a-real-tree-directory-not-a-file-sy-af4f5e",
        )
        .into());
    }
    Ok(Some(parts[2].into()))
}
fn diff(ctx: &Context, a: &str, b: &str) -> Result<String, String> {
    let out = text(
        ctx,
        &[
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--binary",
            a,
            b,
            "--",
        ],
        &[0],
    )?;
    if out.len() > 8 << 20 {
        return Err(crate::i18n::text(
            "backend-subtree-diff-exceeds-the-8-mib-display-limit-narrow-the-folder-be-7823ed",
        )
        .into());
    }
    Ok(out)
}
fn local(ctx: &Context, v: &View, s: &Source) -> Result<Local, String> {
    let active = || !crate::tasks::cancellation().is_cancelled();
    git::drafts::inspection_index(ctx, &active)?;
    let private = |args: &[&str]| -> Result<String, String> {
        String::from_utf8(ctx.work(args, vec![], &[0], &active)?).map_err(|_| {
            crate::i18n::text("backend-subtree-worktree-output-requires-utf-8-310558").into()
        })
    };

    let head_tree = tree(ctx, &v.head, &s.prefix)?;
    let baseline = s.imported.clone().or_else(|| s.fetched.clone());
    let committed = match (&baseline, head_tree) {
        (Some(a), Some(b)) => diff(ctx, &format!("{a}^{{tree}}"), &b)?,
        _ => crate::i18n::text(
            "backend-no-committed-import-source-comparison-yet-fetch-the-registered-so-91eddc",
        )
        .into(),
    };
    let staged = private(&[
        "--literal-pathspecs",
        "diff",
        "--no-ext-diff",
        "--no-textconv",
        "--binary",
        "--cached",
        &v.head,
        "--",
        &s.prefix,
    ])?;
    let working = private(&[
        "--literal-pathspecs",
        "diff",
        "--no-ext-diff",
        "--no-textconv",
        "--binary",
        "--",
        &s.prefix,
    ])?;
    let raw = ctx.work(
        &[
            "--literal-pathspecs",
            "ls-files",
            "--others",
            "--exclude-standard",
            "-z",
            "--",
            &s.prefix,
        ],
        vec![],
        &[0],
        &active,
    )?;
    if staged.len() > 8 << 20 || working.len() > 8 << 20 || raw.len() > 1 << 20 {
        return Err(crate::i18n::text(
            "backend-working-subtree-changes-exceed-the-bounded-display-limit-bb95d2",
        )
        .into());
    }
    let untracked = std::str::from_utf8(&raw)
        .map_err(|_| crate::i18n::text("backend-untracked-subtree-paths-require-utf-8-af9925"))?
        .split('\0')
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if untracked.len() > 2000 {
        return Err(crate::i18n::text(
            "backend-more-than-2000-untracked-subtree-paths-narrow-the-folder-before-r-06b533",
        )
        .into());
    }
    Ok(Local {
        source: s.id.clone(),
        baseline,
        committed,
        staged,
        working,
        untracked,
    })
}
pub(crate) fn read(repo: &Path, selected: &str) -> Result<View, String> {
    let ctx = Context::new(repo, &|| !crate::tasks::cancellation().is_cancelled())?;
    let common = common(&ctx)?;
    let (version, state) = state(&ctx)?;
    validate(&state, &common)?;
    // Read outside the drafts inspection context, which disables signing itself.
    let sign_commits = super::text_codes(
        repo,
        &["config", "--bool", "--get", "commit.gpgSign"],
        &[0, 1],
    )?
    .trim()
        == "true";
    let mut view = View {
        state,
        version,
        branch: branch(&ctx)?,
        head: resolve(&ctx, "HEAD")?,
        remotes: text(&ctx, &["remote"], &[0])?
            .lines()
            .map(str::to_owned)
            .collect(),
        sign_commits,
        local: None,
        candidate_diff: None,
        common,
        route: preflight::outbox_config_stamp(repo)?,
        helper: {
            let exec = PathBuf::from(text(&ctx, &["--exec-path"], &[0])?).join("git-subtree");
            if std::fs::metadata(&exec).is_ok_and(|m| m.is_file()) {
                crate::i18n::message(
                    "backend-installed-helper-file-file-presence-only-execution-capability-unv-c5719d",
                    &[("path", (exec.display()).to_string().into())],
                )
            } else {
                crate::i18n::text("backend-git-subtree-was-not-found-in-git-s-exec-path-install-git-s-contri-3a96ae").into()
            }
        },
    };
    if let Some(c) = view
        .state
        .candidates
        .iter()
        .find(|c| c.id == selected && c.kind != Kind::Split)
        && let Some(result) = &c.result
    {
        view.candidate_diff = Some((c.id.clone(), diff(&ctx, &c.old, result)?));
    }
    let selected_source = view
        .state
        .candidates
        .iter()
        .find(|c| c.id == selected)
        .map(|c| c.source.as_str())
        .unwrap_or(selected);
    if let Some(s) = view
        .state
        .sources
        .iter()
        .find(|s| s.id == selected_source)
        .or_else(|| view.state.sources.first())
    {
        view.local = Some(local(&ctx, &view, s)?);
    }
    Ok(view)
}

#[cfg(all(test, unix))]
mod runtime_tests;
#[cfg(test)]
mod tests;

mod publication;
pub(crate) use publication::prepare_publish;
#[cfg(test)]
use publication::publishable;
#[cfg(test)]
use publication::push_words;
pub(crate) use publication::send;

mod settlement;
pub(crate) use settlement::accept;
#[cfg(test)]
use settlement::accept_allowed;
use settlement::associate;
#[cfg(test)]
use settlement::association_words;
#[cfg(test)]
use settlement::baseline_words;
pub(crate) use settlement::close;
use settlement::finish;
pub(crate) use settlement::reconcile;
pub(crate) use settlement::resume;

mod preparation;
#[cfg(test)]
use preparation::candidate_words;
use preparation::clean_source;
pub(crate) use preparation::fetch;
#[cfg(test)]
use preparation::fetch_words;
use preparation::owned;
pub(crate) use preparation::prepare;
use preparation::route;
#[cfg(test)]
use preparation::subtree_words;

mod registry;
use registry::bytes;
use registry::candidate;
use registry::check;
use registry::common;
use registry::next_id;
pub(crate) use registry::register;
use registry::save;
use registry::source;
use registry::state;
use registry::text;
use registry::validate;
