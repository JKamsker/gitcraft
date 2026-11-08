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
fn validate(r: &Registry, common: &Path) -> Result<(), String> {
    if r.sources.len() > 64 || r.candidates.len() > 64 {
        return Err(crate::i18n::text(
            "backend-subtree-registry-supports-64-sources-and-64-retained-candidates-a-bdc189",
        )
        .into());
    }
    let mut ids = BTreeSet::new();
    for (i, s) in r.sources.iter().enumerate() {
        if !id(&s.id)
            || !ids.insert(&s.id)
            || !prefix(&s.prefix)
            || !plain(&s.remote)
            || !branch_name(&s.branch)
            || r.sources[..i].iter().any(|p| overlap(&p.prefix, &s.prefix))
            || s.fetched.iter().chain(s.imported.iter()).any(|s| !oid(s))
        {
            return Err(crate::i18n::text(
                "backend-invalid-or-overlapping-subtree-source-metadata-858490",
            )
            .into());
        }
    }
    ids.clear();
    for c in &r.candidates {
        let parent = c
            .worktree
            .parent()
            .ok_or(crate::i18n::text("backend-candidate-parent-missing-0cd9e4"))?;
        if !id(&c.id)
            || !ids.insert(&c.id)
            || !r.sources.iter().any(|s| s.id == c.source)
            || !c
                .branch
                .strip_prefix("refs/heads/")
                .is_some_and(branch_name)
            || !oid(&c.old)
            || c.upstream.iter().chain(c.result.iter()).any(|s| !oid(s))
            || c.worktree.file_name().and_then(|s| s.to_str()) != Some("candidate")
            || parent.parent() != Some(common)
            || !parent
                .file_name()
                .and_then(|s| s.to_str())
                .is_some_and(|s| s.starts_with(&format!("gitcraft-subtree-{}-", c.id)))
            || c.identity.as_ref().is_some_and(|s| {
                s.is_empty() || s.len() > 128 || !s.bytes().all(|b| b.is_ascii_digit() || b == b':')
            })
            || c.notice.len() > 65536
        {
            return Err(crate::i18n::text(
                "backend-invalid-retained-subtree-candidate-metadata-3b3282",
            )
            .into());
        }
    }
    Ok(())
}
fn source<'a>(v: &'a View, key: &str) -> Result<&'a Source, String> {
    v.state
        .sources
        .iter()
        .find(|s| s.id == key)
        .ok_or(crate::i18n::text("backend-registered-subtree-disappeared-359d79").into())
}
fn candidate(v: &View, key: &str) -> Result<usize, String> {
    v.state
        .candidates
        .iter()
        .position(|c| c.id == key)
        .ok_or(crate::i18n::text("backend-subtree-candidate-disappeared-a8d155").into())
}
fn bytes(ctx: &Context, args: &[&str], ok: &[i32]) -> Result<Vec<u8>, String> {
    ctx.source(args, vec![], ok, &|| {
        !crate::tasks::cancellation().is_cancelled()
    })
}
fn text(ctx: &Context, args: &[&str], ok: &[i32]) -> Result<String, String> {
    String::from_utf8(bytes(ctx, args, ok)?)
        .map(|s| s.trim_end_matches(['\r', '\n']).to_owned())
        .map_err(|_| crate::i18n::text("backend-subtree-metadata-requires-utf-8-2132f4").into())
}
fn common(ctx: &Context) -> Result<PathBuf, String> {
    if !cfg!(unix) || crate::remote::is_remote(&ctx.repo) {
        return Err(crate::i18n::text(
            "backend-subtree-candidates-currently-require-a-physical-native-unix-check-452d42",
        )
        .into());
    }
    let p = PathBuf::from(text(
        ctx,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        &[0],
    )?);
    if std::fs::canonicalize(&p).map_err(|e| e.to_string())? != p {
        return Err(crate::i18n::text(
            "backend-use-the-physical-repository-path-for-subtree-candidates-41840d",
        )
        .into());
    }
    Ok(p)
}
fn state(ctx: &Context) -> Result<(String, Registry), String> {
    if !text(ctx, &["symbolic-ref", "-q", REF], &[0, 1])?.is_empty() {
        return Err(crate::i18n::text(
            "backend-subtree-state-must-be-a-direct-application-ref-28f144",
        )
        .into());
    }
    let version = text(ctx, &["rev-parse", "--verify", "-q", REF], &[0, 1])?;
    let data = if version.is_empty() {
        Registry::default()
    } else {
        let n = text(ctx, &["cat-file", "-s", &version], &[0])?
            .parse::<usize>()
            .map_err(|_| crate::i18n::text("backend-invalid-subtree-state-size-bec4a1"))?;
        if n > LIMIT {
            return Err(crate::i18n::text("backend-subtree-state-exceeds-2-mib-754414").into());
        }
        serde_json::from_slice(&bytes(ctx, &["cat-file", "blob", &version], &[0])?)
            .map_err(|_| crate::i18n::text("backend-invalid-subtree-registry-json-97f529"))?
    };
    Ok((version, data))
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
    let mut view = View {
        state,
        version,
        branch: branch(&ctx)?,
        head: resolve(&ctx, "HEAD")?,
        remotes: text(&ctx, &["remote"], &[0])?
            .lines()
            .map(str::to_owned)
            .collect(),
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
fn check(repo: &Path, v: &View, head: bool) -> Result<Context, String> {
    let ctx = Context::new(repo, &|| !crate::tasks::cancellation().is_cancelled())?;
    let (version, _) = state(&ctx)?;
    if version != v.version
        || common(&ctx)? != v.common
        || preflight::outbox_config_stamp(repo)? != v.route
        || (head && (branch(&ctx)? != v.branch || resolve(&ctx, "HEAD")? != v.head))
    {
        return Err(crate::i18n::text(
            "backend-subtree-registry-checkout-or-selected-route-changed-read-a-new-vi-e8529f",
        )
        .into());
    }
    Ok(ctx)
}
fn save(repo: &Path, v: &mut View) -> Result<(), String> {
    validate(&v.state, &v.common)?;
    let ctx = check(repo, v, false)?;
    let data = serde_json::to_vec(&v.state).map_err(|e| e.to_string())?;
    if data.len() > LIMIT {
        return Err(crate::i18n::text("backend-subtree-registry-exceeds-2-mib-040d88").into());
    }
    let active = || !crate::tasks::cancellation().is_cancelled();
    let new = ctx.blob(data, &active)?;
    let mut tx = format!(
        "start\noption no-deref\nupdate {REF} {new} {}\n",
        if v.version.is_empty() {
            "0".repeat(new.len())
        } else {
            v.version.clone()
        }
    );
    let mut pins = BTreeSet::new();
    for s in &v.state.sources {
        for o in s.fetched.iter().chain(s.imported.iter()) {
            pins.insert(o.clone());
        }
    }
    for c in &v.state.candidates {
        pins.insert(c.old.clone());
        for o in c.upstream.iter().chain(c.result.iter()) {
            pins.insert(o.clone());
        }
    }
    for o in pins {
        let pin = format!("refs/gitcraft/subtrees/pins/{o}");
        if !text(&ctx, &["symbolic-ref", "-q", &pin], &[0, 1])?.is_empty() {
            return Err(
                crate::i18n::text("backend-subtree-object-pin-became-indirect-a530b9").into(),
            );
        }
        let old = text(&ctx, &["rev-parse", "--verify", "-q", &pin], &[0, 1])?;
        if old.is_empty() {
            tx.push_str(&format!("option no-deref\ncreate {pin} {o}\n"));
        } else if old != o {
            return Err(crate::i18n::text(
                "backend-an-immutable-subtree-object-pin-changed-ad53fb",
            )
            .into());
        } else {
            tx.push_str(&format!("option no-deref\nverify {pin} {o}\n"));
        }
    }
    tx.push_str("prepare\ncommit\n");
    // On lost acknowledgement, stop with all work retained. Never roll back a possible publication.
    ctx.source(
        &["update-ref", "-m", "gitcraft subtree state", "--stdin"],
        tx.into_bytes(),
        &[0],
        &active,
    )?;
    v.version = new;
    Ok(())
}
fn next_id() -> String {
    static IDS: AtomicU64 = AtomicU64::new(0);
    format!(
        "{}-{}-{}",
        git::now(),
        std::process::id(),
        IDS.fetch_add(1, Ordering::Relaxed)
    )
}
fn route(repo: &Path, s: &Source) -> Result<(), String> {
    for flags in [
        vec!["remote", "get-url", "--all", "--", s.remote.as_str()],
        vec![
            "remote",
            "get-url",
            "--push",
            "--all",
            "--",
            s.remote.as_str(),
        ],
    ] {
        let urls = super::guard::text(repo, &flags)?;
        if urls.is_empty() {
            return Err(
                crate::i18n::text("backend-registered-source-has-no-endpoint-bbd38e").into(),
            );
        }
        for url in urls.lines() {
            validate_url(url)?;
        }
    }
    Ok(())
}
pub(crate) fn register(
    r: &Runner<'_>,
    v: &View,
    path: String,
    remote: String,
    branch: String,
    history: History,
) -> Result<String, String> {
    let ctx = check(r.repo, v, true)?;
    if !prefix(&path)
        || !v.remotes.contains(&remote)
        || !plain(&branch)
        || v.state.sources.iter().any(|s| overlap(&path, &s.prefix))
    {
        return Err(crate::i18n::text(
            "backend-choose-a-non-overlapping-relative-folder-configured-remote-and-br-814b4a",
        )
        .into());
    }
    bytes(
        &ctx,
        &["check-ref-format", &format!("refs/heads/{branch}")],
        &[0],
    )?;
    tree(&ctx, &v.head, &path)?;
    let s = Source {
        id: next_id(),
        prefix: path,
        remote,
        branch,
        history,
        fetched: None,
        imported: None,
    };
    route(r.repo, &s)?;
    let mut view = v.clone();
    view.state.sources.push(s);
    save(r.repo, &mut view)?;
    Ok(crate::i18n::text(
        "backend-subtree-source-registered-locally-no-helper-or-network-operation-c8a7e5",
    )
    .into())
}
fn fetch_words(s: &Source, pin: &str) -> Vec<String> {
    [
        "-c",
        "fetch.recurseSubmodules=false",
        "-c",
        "fetch.writeCommitGraph=false",
        "-c",
        "gc.auto=0",
        "-c",
        "maintenance.auto=false",
        "fetch",
        "--no-tags",
        "--no-recurse-submodules",
        "--no-write-fetch-head",
        "--",
        &s.remote,
        &format!("refs/heads/{}:{pin}", s.branch),
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}
pub(crate) fn fetch(r: &Runner<'_>, v: &View, key: &str) -> Result<String, String> {
    check(r.repo, v, true)?;
    let s = source(v, key)?.clone();
    route(r.repo, &s)?;
    if let Some(why) = crate::network::refused() {
        return Err(why.why().into());
    }
    let pin = format!("refs/gitcraft/subtrees/fetch/{}", next_id());
    let ctx = check(r.repo, v, true)?;
    if !text(&ctx, &["rev-parse", "--verify", "-q", &pin], &[0, 1])?.is_empty() {
        return Err(crate::i18n::text(
            "backend-private-subtree-fetch-ref-already-exists-no-fetch-attempted-bd251e",
        )
        .into());
    }
    let args = fetch_words(&s, &pin);
    r.run(&args.iter().map(String::as_str).collect::<Vec<_>>())?;
    let ctx = check(r.repo, v, true)?;
    let result = resolve(&ctx, &pin)?;
    let mut view = v.clone();
    view.state
        .sources
        .iter_mut()
        .find(|s| s.id == key)
        .ok_or(crate::i18n::text("backend-subtree-disappeared-1c5e16"))?
        .fetched = Some(result);
    save(r.repo, &mut view)?;
    Ok(crate::i18n::message(
        "backend-fetched-the-registered-branch-into-pin-original-branch-index-unch-367bf0",
        &[("pin", (pin).to_string().into())],
    ))
}
fn subtree_words(
    s: &Source,
    kind: Kind,
    old: &str,
    upstream: Option<&str>,
    exists: bool,
) -> Result<Vec<String>, String> {
    let mut a = vec![
        "subtree".into(),
        match kind {
            Kind::Split => "split",
            Kind::Associate => {
                return Err(
                    crate::i18n::text("backend-association-uses-immutable-metadata-commits-not-a-subtree-helper-f977d8").into(),
                );
            }
            Kind::Update if exists => "merge",
            Kind::Update => "add",
        }
        .into(),
        format!("--prefix={}", s.prefix),
    ];
    if kind == Kind::Update {
        if s.history == History::Squash {
            a.push("--squash".into());
        }
        a.push(
            upstream
                .filter(|s| oid(s))
                .ok_or(crate::i18n::text(
                    "backend-fetch-a-source-commit-before-preparing-an-import-587a9b",
                ))?
                .into(),
        );
    } else {
        a.push(old.into());
    }
    Ok(a)
}
fn owned(r: &Runner<'_>, v: &View, c: &Candidate, args: &[&str]) -> Result<String, String> {
    deferred::registered_run(
        r,
        &v.common,
        &c.worktree,
        c.identity.as_deref().ok_or(crate::i18n::text(
            "backend-creation-outcome-is-unacknowledged-candidate-cannot-be-adopted-f78ef5",
        ))?,
        args,
    )
}
fn clean_source(ctx: &Context) -> Result<(), String> {
    let active = || !crate::tasks::cancellation().is_cancelled();
    git::drafts::inspection_index(ctx, &active)?;
    let head = resolve(ctx, "HEAD")?;
    let staged = ctx.run(
        &[
            "diff",
            "--cached",
            "--no-ext-diff",
            "--no-textconv",
            "--name-only",
            &head,
            "--",
        ],
        vec![],
        &[0],
        &active,
    )?;
    let work = ctx.work(
        &[
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--name-only",
            "--",
        ],
        vec![],
        &[0],
        &active,
    )?;
    let untracked = ctx.work(&["ls-files", "--others", "-z"], vec![], &[0], &active)?;
    if !staged.is_empty() || !work.is_empty() || !untracked.is_empty() {
        return Err(crate::i18n::text(
            "backend-original-checkout-must-have-no-staged-unstaged-or-untracked-files-b562d4",
        )
        .into());
    }
    Ok(())
}
pub(crate) fn prepare(r: &Runner<'_>, v: &View, key: &str, kind: Kind) -> Result<String, String> {
    let ctx = check(r.repo, v, true)?;
    let s = source(v, key)?.clone();
    if !v.branch.starts_with("refs/heads/")
        || !text(&ctx, &["symbolic-ref", "-q", &v.branch], &[0, 1])?.is_empty()
    {
        return Err(crate::i18n::text(
            "backend-select-a-direct-local-branch-before-preparing-a-subtree-candidate-364b76",
        )
        .into());
    }
    let exists = tree(&ctx, &v.head, &s.prefix)?.is_some();
    if kind == Kind::Split && !exists {
        return Err(crate::i18n::text(
            "backend-commit-a-vendored-folder-before-splitting-its-history-994603",
        )
        .into());
    }
    let args = if kind == Kind::Associate {
        if !exists || s.imported.is_some() || s.fetched.is_none() {
            return Err(crate::i18n::text(
                "backend-association-requires-an-existing-folder-reviewed-fetched-baseline-8325da",
            )
            .into());
        }
        if v.local
            .as_ref()
            .is_none_or(|l| l.source != s.id || l.baseline != s.fetched)
        {
            return Err(crate::i18n::text(
                "backend-inspect-this-folder-against-the-exact-fetched-baseline-before-ass-b4af92",
            )
            .into());
        }
        Vec::new()
    } else {
        subtree_words(&s, kind, &v.head, s.fetched.as_deref(), exists)?
    };
    if v.state.candidates.len() >= 64 {
        return Err(crate::i18n::text(
            "backend-64-candidates-retained-explicitly-archive-reviewed-metadata-befor-1eab61",
        )
        .into());
    }
    let id = next_id();
    let parent = tempfile::Builder::new()
        .prefix(&format!("gitcraft-subtree-{id}-"))
        .tempdir_in(&v.common)
        .map_err(|e| e.to_string())?
        .keep();
    let worktree = parent.join("candidate");
    let mut view = v.clone();
    view.state.candidates.push(Candidate {
        id: id.clone(),
        source: key.into(),
        branch: v.branch.clone(),
        old: v.head.clone(),
        upstream: if kind != Kind::Split {
            s.fetched.clone()
        } else {
            None
        },
        kind,
        stage: Stage::Preparing,
        worktree: worktree.clone(),
        identity: None,
        result: None,
        notice: crate::i18n::text(
            "backend-creation-intent-persisted-if-acknowledgement-is-lost-this-directo-00e4cd",
        )
        .into(),
    });
    save(r.repo, &mut view)?; // Before any helper/ref/working-folder operation.
    if std::fs::symlink_metadata(&worktree).is_ok() {
        return Err(crate::i18n::text(
            "backend-candidate-path-appeared-no-existing-directory-was-adopted-3ed7f8",
        )
        .into());
    }
    check(r.repo, &view, true)?;
    r.run(&[
        "-c",
        "core.hooksPath=/dev/null",
        "-c",
        "core.fsmonitor=false",
        "-c",
        "submodule.recurse=false",
        "worktree",
        "add",
        "--detach",
        "--lock",
        "--reason",
        "gitcraft retained subtree candidate",
        "--",
        worktree.to_str().ok_or(crate::i18n::text(
            "backend-candidate-path-requires-utf-8-df8c79",
        ))?,
        &v.head,
    ])?;
    let i = candidate(&view, &id)?;
    view.state.candidates[i].identity =
        Some(deferred::registered_identity(r.repo, &v.common, &worktree)?);
    view.state.candidates[i].stage = Stage::Running;
    view.state.candidates[i].notice = crate::i18n::text(
        "backend-helper-dispatch-recorded-an-interrupted-operation-requires-inspec-07f6aa",
    )
    .into();
    save(r.repo, &mut view)?;
    let c = view.state.candidates[i].clone();
    let result = if kind == Kind::Associate {
        associate(r, &view, &c, &s)
    } else {
        owned(
            r,
            &view,
            &c,
            &args.iter().map(String::as_str).collect::<Vec<_>>(),
        )
    };
    finish(r, &mut view, i, result)?;
    Ok(crate::i18n::message(
        "backend-subtree-candidate-id-retained-at-refresh-to-inspect-its-result-be-4e0ada",
        &[
            ("id", (id).to_string().into()),
            ("path", (worktree.display()).to_string().into()),
        ],
    ))
}
fn association_message(prefix: &str, old: &str, base: &str) -> String {
    format!(
        "Associate vendored folder with explicitly reviewed upstream baseline; keep all folder contents\n\ngit-subtree-dir: {prefix}\ngit-subtree-mainline: {old}\ngit-subtree-split: {base}\n"
    )
}
fn baseline_words(s: &Source, base: &str) -> Option<Vec<String>> {
    (s.history == History::Squash).then(|| {
        vec![
            "commit-tree".into(),
            format!("{base}^{{tree}}"),
            "-m".into(),
            format!(
                "Squashed baseline for {}\n\ngit-subtree-dir: {}\ngit-subtree-split: {base}\n",
                s.prefix, s.prefix
            ),
        ]
    })
}
fn association_words(c: &Candidate, s: &Source, parent: &str, base: &str) -> Vec<String> {
    vec![
        "commit-tree".into(),
        format!("{}^{{tree}}", c.old),
        "-p".into(),
        c.old.clone(),
        "-p".into(),
        parent.into(),
        "-m".into(),
        association_message(&s.prefix, &c.old, base),
    ]
}
fn associate(r: &Runner<'_>, v: &View, c: &Candidate, s: &Source) -> Result<String, String> {
    let base = c.upstream.as_deref().ok_or(crate::i18n::text(
        "backend-no-immutable-association-baseline-301ce5",
    ))?;
    let parent = if let Some(args) = baseline_words(s, base) {
        owned(
            r,
            v,
            c,
            &args.iter().map(String::as_str).collect::<Vec<_>>(),
        )?
        .trim()
        .to_owned()
    } else {
        base.to_owned()
    };
    if !oid(&parent) {
        return Err(crate::i18n::text(
            "backend-invalid-association-parent-retained-without-updating-candidate-he-6819eb",
        )
        .into());
    }
    let args = association_words(c, s, &parent, base);
    let result = owned(
        r,
        v,
        c,
        &args.iter().map(String::as_str).collect::<Vec<_>>(),
    )?
    .trim()
    .to_owned();
    if !oid(&result) {
        return Err(crate::i18n::text(
            "backend-invalid-association-result-retained-without-updating-candidate-he-583370",
        )
        .into());
    }
    owned(
        r,
        v,
        c,
        &["update-ref", "--no-deref", "HEAD", &result, &c.old],
    )?;
    Ok(result)
}
fn finish(
    r: &Runner<'_>,
    v: &mut View,
    i: usize,
    result: Result<String, String>,
) -> Result<(), String> {
    let c = v.state.candidates[i].clone();
    let unmerged = owned(
        r,
        v,
        &c,
        &[
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--name-only",
            "--diff-filter=U",
            "--",
        ],
    )?;
    let status = owned(
        r,
        v,
        &c,
        &["status", "--porcelain=v1", "--untracked-files=all"],
    )?;
    match result {
        Ok(out) if unmerged.is_empty() && status.is_empty() => {
            let value = if c.kind == Kind::Split {
                out.lines()
                    .rev()
                    .find(|s| oid(s))
                    .ok_or(crate::i18n::text(
                        "backend-subtree-split-did-not-return-an-immutable-commit-e9d2fb",
                    ))?
                    .into()
            } else {
                owned(r, v, &c, &["rev-parse", "--verify", "HEAD"])?
            };
            let value = value.trim().to_owned();
            if !oid(&value) {
                return Err(crate::i18n::text(
                    "backend-invalid-subtree-result-retained-for-inspection-642ff8",
                )
                .into());
            }
            if c.kind == Kind::Split {
                let s = source(v, &c.source)?;
                let expected = owned(
                    r,
                    v,
                    &c,
                    &["rev-parse", "--verify", &format!("{}:{}", c.old, s.prefix)],
                )?;
                let actual = owned(
                    r,
                    v,
                    &c,
                    &["rev-parse", "--verify", &format!("{value}^{{tree}}")],
                )?;
                if expected.trim() != actual.trim() {
                    return Err(crate::i18n::text("backend-split-result-tree-differs-from-the-selected-committed-folder-reta-816a29").into());
                }
            }
            v.state.candidates[i].result = Some(value);
            v.state.candidates[i].stage = if c.kind == Kind::Split {
                Stage::SplitReady
            } else {
                Stage::Ready
            };
            v.state.candidates[i].notice = crate::i18n::text(
                "backend-immutable-result-retained-original-checkout-and-branch-were-not-c-3f0680",
            )
            .into();
        }
        err => {
            v.state.candidates[i].stage = if !unmerged.is_empty() {
                Stage::Conflict
            } else {
                Stage::Paused
            };
            v.state.candidates[i].notice = crate::i18n::message(
                "backend-unmerged-candidate-status-inspect-retained-work-no-original-branc-5a2435",
                &[
                    (
                        "git",
                        (git::redact(
                            &err.err().unwrap_or_else(|| {
                                crate::i18n::text("backend-helper-left-working-changes-edf853")
                                    .into()
                            }),
                            None,
                        ))
                        .to_string()
                        .into(),
                    ),
                    ("unmerged", (unmerged).to_string().into()),
                    ("status", (status).to_string().into()),
                ],
            );
            if v.state.candidates[i].notice.len() > 65536 {
                v.state.candidates[i].notice =
                    v.state.candidates[i].notice.chars().take(12000).collect();
            }
        }
    }
    save(r.repo, v)
}
pub(crate) fn resume(r: &Runner<'_>, v: &View, key: &str) -> Result<String, String> {
    check(r.repo, v, false)?;
    let i = candidate(v, key)?;
    let c = v.state.candidates[i].clone();
    if c.kind != Kind::Update
        || !matches!(c.stage, Stage::Conflict | Stage::Running | Stage::Paused)
    {
        return Err(crate::i18n::text(
            "backend-only-an-inspected-retained-import-may-be-continued-9a9ccb",
        )
        .into());
    }
    if !owned(
        r,
        v,
        &c,
        &[
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--name-only",
            "--diff-filter=U",
            "--",
        ],
    )?
    .is_empty()
    {
        return Err(crate::i18n::text(
            "backend-resolve-and-stage-every-candidate-conflict-before-continuing-orig-007c3a",
        )
        .into());
    }
    let head = owned(r, v, &c, &["rev-parse", "--verify", "HEAD"])?;
    let pending = owned(r, v, &c, &["rev-parse", "--verify", "-q", "MERGE_HEAD"]);
    let mut view = v.clone();
    view.state.candidates[i].stage = Stage::Running;
    save(r.repo, &mut view)?;
    let result = if pending.is_ok() {
        owned(r, &view, &c, &["commit", "--no-edit", "--no-verify"])
    } else if head.trim() != c.old {
        Ok(String::new())
    } else {
        Err(crate::i18n::text(
            "backend-no-acknowledged-merge-result-exists-inspect-this-candidate-then-p-9fc5b1",
        )
        .into())
    };
    finish(r, &mut view, i, result)?;
    Ok(crate::i18n::text(
        "backend-candidate-inspected-refresh-to-review-the-immutable-result-fab422",
    )
    .into())
}
fn accept_allowed(c: &Candidate, branch: &str, head: &str, clean: bool) -> Result<(), String> {
    if c.stage != Stage::Ready
        || c.kind == Kind::Split
        || c.result.is_none()
        || branch != c.branch
        || head != c.old
        || !clean
    {
        return Err(crate::i18n::text(
            "backend-acceptance-requires-the-reviewed-direct-branch-at-its-original-co-723f86",
        )
        .into());
    }
    Ok(())
}
pub(crate) fn accept(r: &Runner<'_>, v: &View, key: &str) -> Result<String, String> {
    let ctx = check(r.repo, v, true)?;
    let i = candidate(v, key)?;
    let c = v.state.candidates[i].clone();
    clean_source(&ctx)?;
    accept_allowed(&c, &v.branch, &v.head, true)?;
    if v.candidate_diff.as_ref().map(|(id, _)| id.as_str()) != Some(key) {
        return Err(crate::i18n::text(
            "backend-review-this-exact-candidate-patch-before-accepting-c38089",
        )
        .into());
    }
    if owned(r, v, &c, &["rev-parse", "--verify", "HEAD"])?.trim()
        != c.result.as_deref().unwrap_or("")
        || !owned(
            r,
            v,
            &c,
            &["status", "--porcelain=v1", "--untracked-files=all"],
        )?
        .is_empty()
    {
        return Err(crate::i18n::text(
            "backend-candidate-changed-refresh-and-review-it-before-accepting-91448b",
        )
        .into());
    }
    let mut view = v.clone();
    view.state.candidates[i].stage = Stage::Accepting;
    save(r.repo, &mut view)?;
    // Shared history transaction owns native HEAD/ref/index/checkout guards and recovery.
    // Failure retains Accepting; no materialization rollback or blind acceptance retry.
    preflight::accept_rewrite(
        r.repo,
        &c.branch,
        &c.old,
        c.result
            .as_deref()
            .ok_or(crate::i18n::text("backend-no-result-c6008b"))?,
    )?;
    view.state.candidates[i].stage = Stage::Accepted;
    view.state
        .sources
        .iter_mut()
        .find(|s| s.id == c.source)
        .ok_or(crate::i18n::text("backend-source-disappeared-5ff00e"))?
        .imported = c.upstream;
    save(r.repo, &mut view)?;
    Ok(crate::i18n::text(
        "backend-subtree-update-accepted-through-the-shared-guarded-checkout-trans-ec2bef",
    )
    .into())
}
pub(crate) fn reconcile(r: &Runner<'_>, v: &View, key: &str) -> Result<String, String> {
    let ctx = check(r.repo, v, false)?;
    let i = candidate(v, key)?;
    let c = &v.state.candidates[i];
    if c.stage != Stage::Accepting {
        return Err(crate::i18n::text(
            "backend-only-an-uncertain-acceptance-can-be-reconciled-here-inspect-ambig-a6dce1",
        )
        .into());
    }
    clean_source(&ctx)?;
    let head = resolve(&ctx, "HEAD")?;
    let branch = branch(&ctx)?;
    let stage = if branch == c.branch && Some(head.as_str()) == c.result.as_deref() {
        Stage::Accepted
    } else if branch == c.branch && head == c.old {
        Stage::Ready
    } else {
        return Err(crate::i18n::text(
            "backend-original-checkout-differs-from-both-exact-acceptance-receipts-ret-e5ab48",
        )
        .into());
    };
    let mut view = v.clone();
    view.state.candidates[i].stage = stage;
    if stage == Stage::Accepted {
        view.state
            .sources
            .iter_mut()
            .find(|s| s.id == c.source)
            .ok_or(crate::i18n::text("backend-source-disappeared-5ff00e"))?
            .imported = c.upstream.clone();
    }
    save(r.repo, &mut view)?;
    Ok(crate::i18n::text(
        "backend-exact-original-checkout-receipt-reconciled-no-branch-or-working-f-50d6bd",
    )
    .into())
}
pub(crate) fn close(r: &Runner<'_>, v: &View, key: &str) -> Result<String, String> {
    check(r.repo, v, false)?;
    let i = candidate(v, key)?;
    if matches!(
        v.state.candidates[i].stage,
        Stage::Accepting | Stage::Sending
    ) {
        return Err(crate::i18n::text(
            "backend-inspect-reconcile-the-uncertain-outcome-before-closing-its-metada-1a17fe",
        )
        .into());
    }
    let mut view = v.clone();
    view.state.candidates[i].stage = Stage::Closed;
    save(r.repo, &mut view)?;
    Ok(crate::i18n::text(
        "backend-proposal-closed-its-worktree-results-and-pins-remain-retained-for-d32620",
    )
    .into())
}
fn principal(c: Option<&git::Credentials>) -> Result<Option<(String, String, String)>, String> {
    c.map(|c| {
        let s = c.scope.as_ref().ok_or(crate::i18n::text(
            "backend-sign-in-lacks-endpoint-account-scope-sign-in-again-71595d",
        ))?;
        Ok((s.endpoint(), s.account().into(), c.username.clone()))
    })
    .transpose()
}
fn publishable(c: &Candidate, renewed: bool) -> bool {
    c.kind == Kind::Split
        && c.result.is_some()
        && (c.stage == Stage::SplitReady
            || (renewed && matches!(c.stage, Stage::Sending | Stage::Paused)))
}
fn push_words(s: &Source, c: &Candidate) -> Result<Vec<Vec<String>>, String> {
    Ok(vec![vec![
        "push".into(),
        "--porcelain".into(),
        "--no-follow-tags".into(),
        "--recurse-submodules=check".into(),
        "--".into(),
        s.remote.clone(),
        format!(
            "{}:refs/heads/{}",
            c.result
                .as_deref()
                .ok_or(crate::i18n::text("backend-split-result-missing-1948a0"))?,
            s.branch
        ),
    ]])
}
pub(crate) fn prepare_publish(
    repo: &Path,
    v: &View,
    key: &str,
    profile: UserProfile,
    credentials: Option<&git::Credentials>,
    generation: u64,
    renewed: bool,
) -> Result<Prepared, String> {
    check(repo, v, true)?;
    let c = &v.state.candidates[candidate(v, key)?];
    if !publishable(c, renewed) {
        return Err(crate::i18n::text(
            "backend-split-is-not-ready-uncertain-rejected-sends-require-explicit-remo-73d014",
        )
        .into());
    }
    let context = crate::connections::current();
    if context.error.is_some() || context.choices != profile.connections {
        return Err(crate::i18n::text(
            "backend-selected-profile-connection-changed-prepare-again-b5de52",
        )
        .into());
    }
    let s = source(v, &c.source)?;
    route(repo, s)?;
    let plan = preflight::scan(repo, push_words(s, c)?, profile, &|| {
        !crate::tasks::cancellation().is_cancelled()
    })?;
    if Some(plan.head.as_str()) != c.result.as_deref()
        || plan.remote != s.remote
        || plan.destination != format!("refs/heads/{}", s.branch)
        || plan.force
        || plan.deleting
    {
        return Err(crate::i18n::text(
            "backend-publication-preflight-differs-from-the-registered-exact-split-des-11eaf6",
        )
        .into());
    }
    Ok(Prepared {
        view: v.clone(),
        id: key.into(),
        plan,
        context,
        principal: principal(credentials)?,
        generation,
        renewed,
    })
}
pub(crate) fn send(
    r: &Runner<'_>,
    p: &Prepared,
    reviewed: &preflight::ReviewedSend,
) -> Result<String, String> {
    reviewed.check_plan(&p.plan)?;
    let ctx = check(r.repo, &p.view, true)?;
    let _ = ctx;
    if p.context != crate::connections::current() || p.principal != principal(r.credentials)? {
        return Err(crate::i18n::text(
            "backend-prepared-connection-account-changed-prepare-and-approve-again-975afc",
        )
        .into());
    }
    let i = candidate(&p.view, &p.id)?;
    let c = &p.view.state.candidates[i];
    if !publishable(c, p.renewed) {
        return Err(
            crate::i18n::text("backend-split-no-longer-eligible-for-this-approval-51ae0f").into(),
        );
    }
    preflight::check(r.repo, &p.plan)?;
    let mut view = p.view.clone();
    view.state.candidates[i].stage = Stage::Sending;
    view.state.candidates[i].notice = crate::i18n::text(
        "backend-network-dispatch-intent-persisted-missing-rejected-receipt-never-786593",
    )
    .into();
    save(r.repo, &mut view)?;
    let result = preflight::execute_bound_controlled(
        r.repo,
        &p.plan,
        reviewed,
        r.credentials,
        Some(r.control),
    );
    view.state.candidates[i].stage = if result.is_ok() {
        Stage::Sent
    } else {
        Stage::Paused
    };
    view.state.candidates[i].notice = result
        .as_ref()
        .map(|_| {
            crate::i18n::text(
                "backend-exact-split-push-acknowledged-local-candidate-pins-retained-557cac",
            )
            .into()
        })
        .unwrap_or_else(|e| {
            crate::i18n::message(
                "backend-push-rejected-or-uncertain-inspect-destination-before-renewing-ap-8dc126",
                &[("git", (git::redact(e, None)).to_string().into())],
            )
        });
    save(r.repo, &mut view)?;
    result
}
#[cfg(all(test, unix))]
mod runtime_tests;
#[cfg(test)]
mod tests;
