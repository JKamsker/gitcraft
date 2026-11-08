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
const HISTORY: &str = "Squashed imports add a condensed upstream commit; full-history imports retain upstream ancestry. Both keep local folder edits and can conflict. This choice controls future imports; registering an existing folder does not convert its old history. Split reconstructs folder-only history from the selected committed HEAD; staged, unstaged and untracked work is shown separately and is not published.";
pub(crate) fn history_explanation() -> &'static str {
    HISTORY
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
        return Err("Subtree registry supports 64 sources and 64 retained candidates; archive reviewed metadata explicitly before adding more".into());
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
            return Err("Invalid or overlapping subtree source metadata".into());
        }
    }
    ids.clear();
    for c in &r.candidates {
        let parent = c.worktree.parent().ok_or("Candidate parent missing")?;
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
            return Err("Invalid retained subtree candidate metadata".into());
        }
    }
    Ok(())
}
fn source<'a>(v: &'a View, key: &str) -> Result<&'a Source, String> {
    v.state
        .sources
        .iter()
        .find(|s| s.id == key)
        .ok_or("Registered subtree disappeared".into())
}
fn candidate(v: &View, key: &str) -> Result<usize, String> {
    v.state
        .candidates
        .iter()
        .position(|c| c.id == key)
        .ok_or("Subtree candidate disappeared".into())
}
fn bytes(ctx: &Context, args: &[&str], ok: &[i32]) -> Result<Vec<u8>, String> {
    ctx.source(args, vec![], ok, &|| {
        !crate::tasks::cancellation().is_cancelled()
    })
}
fn text(ctx: &Context, args: &[&str], ok: &[i32]) -> Result<String, String> {
    String::from_utf8(bytes(ctx, args, ok)?)
        .map(|s| s.trim_end_matches(['\r', '\n']).to_owned())
        .map_err(|_| "Subtree metadata requires UTF-8".into())
}
fn common(ctx: &Context) -> Result<PathBuf, String> {
    if !cfg!(unix) || crate::remote::is_remote(&ctx.repo) {
        return Err("Subtree candidates currently require a physical native Unix checkout; no helper was dispatched".into());
    }
    let p = PathBuf::from(text(
        ctx,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        &[0],
    )?);
    if std::fs::canonicalize(&p).map_err(|e| e.to_string())? != p {
        return Err("Use the physical repository path for subtree candidates".into());
    }
    Ok(p)
}
fn state(ctx: &Context) -> Result<(String, Registry), String> {
    if !text(ctx, &["symbolic-ref", "-q", REF], &[0, 1])?.is_empty() {
        return Err("Subtree state must be a direct application ref".into());
    }
    let version = text(ctx, &["rev-parse", "--verify", "-q", REF], &[0, 1])?;
    let data = if version.is_empty() {
        Registry::default()
    } else {
        let n = text(ctx, &["cat-file", "-s", &version], &[0])?
            .parse::<usize>()
            .map_err(|_| "Invalid subtree state size")?;
        if n > LIMIT {
            return Err("Subtree state exceeds 2 MiB".into());
        }
        serde_json::from_slice(&bytes(ctx, &["cat-file", "blob", &version], &[0])?)
            .map_err(|_| "Invalid subtree registry JSON")?
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
        return Err("Invalid immutable subtree commit".into());
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
    let s = std::str::from_utf8(&raw).map_err(|_| "Invalid vendored tree metadata")?;
    let (meta, name) = s
        .trim_end_matches('\0')
        .split_once('\t')
        .ok_or("Invalid vendored tree")?;
    let parts = meta.split(' ').collect::<Vec<_>>();
    if parts.len() != 3
        || parts[0] != "040000"
        || parts[1] != "tree"
        || name != path
        || !oid(parts[2])
    {
        return Err(
            "The registered prefix must be a real tree directory, not a file, symlink or submodule"
                .into(),
        );
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
        return Err(
            "Subtree diff exceeds the 8 MiB display limit; narrow the folder before reviewing"
                .into(),
        );
    }
    Ok(out)
}
fn local(ctx: &Context, v: &View, s: &Source) -> Result<Local, String> {
    let active = || !crate::tasks::cancellation().is_cancelled();
    git::drafts::inspection_index(ctx, &active)?;
    let private = |args: &[&str]| -> Result<String, String> {
        String::from_utf8(ctx.work(args, vec![], &[0], &active)?)
            .map_err(|_| "Subtree worktree output requires UTF-8".into())
    };

    let head_tree = tree(ctx, &v.head, &s.prefix)?;
    let baseline = s.imported.clone().or_else(|| s.fetched.clone());
    let committed=match (&baseline,head_tree) {
        (Some(a),Some(b))=>diff(ctx,&format!("{a}^{{tree}}"),&b)?,
        _=>"No committed import/source comparison yet. Fetch the registered source explicitly, or prepare its first import.".into(),
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
        return Err("Working subtree changes exceed the bounded display limit".into());
    }
    let untracked = std::str::from_utf8(&raw)
        .map_err(|_| "Untracked subtree paths require UTF-8")?
        .split('\0')
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if untracked.len() > 2000 {
        return Err(
            "More than 2000 untracked subtree paths; narrow the folder before reviewing".into(),
        );
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
                format!(
                    "Installed helper file: {} (file presence only; execution/capability unverified)",
                    exec.display()
                )
            } else {
                "git-subtree was not found in Git's exec path. Install Git's contrib subtree helper before preparing an update/split; a PATH-provided helper is attempted only by your explicit Prepare action".into()
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
        return Err("Subtree registry, checkout or selected route changed; read a new view".into());
    }
    Ok(ctx)
}
fn save(repo: &Path, v: &mut View) -> Result<(), String> {
    validate(&v.state, &v.common)?;
    let ctx = check(repo, v, false)?;
    let data = serde_json::to_vec(&v.state).map_err(|e| e.to_string())?;
    if data.len() > LIMIT {
        return Err("Subtree registry exceeds 2 MiB".into());
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
            return Err("Subtree object pin became indirect".into());
        }
        let old = text(&ctx, &["rev-parse", "--verify", "-q", &pin], &[0, 1])?;
        if old.is_empty() {
            tx.push_str(&format!("option no-deref\ncreate {pin} {o}\n"));
        } else if old != o {
            return Err("An immutable subtree object pin changed".into());
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
            return Err("Registered source has no endpoint".into());
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
        return Err(
            "Choose a non-overlapping relative folder, configured remote and branch".into(),
        );
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
    Ok("Subtree source registered locally; no helper or network operation ran".into())
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
        return Err("Private subtree fetch ref already exists; no fetch attempted".into());
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
        .ok_or("Subtree disappeared")?
        .fetched = Some(result);
    save(r.repo, &mut view)?;
    Ok(format!(
        "Fetched the registered branch into {pin}; original branch/index unchanged. Refresh to inspect local edits or prepare its import"
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
                    "Association uses immutable metadata commits, not a subtree helper".into(),
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
                .ok_or("Fetch a source commit before preparing an import")?
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
        c.identity
            .as_deref()
            .ok_or("Creation outcome is unacknowledged; candidate cannot be adopted")?,
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
        return Err("Original checkout must have no staged, unstaged or untracked files (including ignored files) before accepting; your work was retained".into());
    }
    Ok(())
}
pub(crate) fn prepare(r: &Runner<'_>, v: &View, key: &str, kind: Kind) -> Result<String, String> {
    let ctx = check(r.repo, v, true)?;
    let s = source(v, key)?.clone();
    if !v.branch.starts_with("refs/heads/")
        || !text(&ctx, &["symbolic-ref", "-q", &v.branch], &[0, 1])?.is_empty()
    {
        return Err("Select a direct local branch before preparing a subtree candidate".into());
    }
    let exists = tree(&ctx, &v.head, &s.prefix)?.is_some();
    if kind == Kind::Split && !exists {
        return Err("Commit a vendored folder before splitting its history".into());
    }
    let args = if kind == Kind::Associate {
        if !exists || s.imported.is_some() || s.fetched.is_none() {
            return Err("Association requires an existing folder, reviewed fetched baseline and no prior accepted import".into());
        }
        if v.local
            .as_ref()
            .is_none_or(|l| l.source != s.id || l.baseline != s.fetched)
        {
            return Err(
                "Inspect this folder against the exact fetched baseline before associating it"
                    .into(),
            );
        }
        Vec::new()
    } else {
        subtree_words(&s, kind, &v.head, s.fetched.as_deref(), exists)?
    };
    if v.state.candidates.len() >= 64 {
        return Err(
            "64 candidates retained; explicitly archive reviewed metadata before preparing more"
                .into(),
        );
    }
    let id = next_id();
    let parent = tempfile::Builder::new()
        .prefix(&format!("gitcraft-subtree-{id}-"))
        .tempdir_in(&v.common)
        .map_err(|e| e.to_string())?
        .keep();
    let worktree = parent.join("candidate");
    let mut view = v.clone();
    view.state.candidates.push(Candidate{id:id.clone(),source:key.into(),branch:v.branch.clone(),old:v.head.clone(),upstream:if kind!=Kind::Split{s.fetched.clone()}else{None},kind,stage:Stage::Preparing,worktree:worktree.clone(),identity:None,result:None,notice:"Creation intent persisted. If acknowledgement is lost, this directory is never automatically adopted or removed".into()});
    save(r.repo, &mut view)?; // Before any helper/ref/working-folder operation.
    if std::fs::symlink_metadata(&worktree).is_ok() {
        return Err("Candidate path appeared; no existing directory was adopted".into());
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
        worktree.to_str().ok_or("Candidate path requires UTF-8")?,
        &v.head,
    ])?;
    let i = candidate(&view, &id)?;
    view.state.candidates[i].identity =
        Some(deferred::registered_identity(r.repo, &v.common, &worktree)?);
    view.state.candidates[i].stage = Stage::Running;
    view.state.candidates[i].notice="Helper dispatch recorded; an interrupted operation requires inspection/reconciliation, never automatic retry".into();
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
    Ok(format!(
        "Subtree candidate {id} retained at {}. Refresh to inspect its result before accepting or publishing",
        worktree.display()
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
    let base = c
        .upstream
        .as_deref()
        .ok_or("No immutable association baseline")?;
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
        return Err("Invalid association parent; retained without updating candidate HEAD".into());
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
        return Err("Invalid association result; retained without updating candidate HEAD".into());
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
                    .ok_or("Subtree split did not return an immutable commit")?
                    .into()
            } else {
                owned(r, v, &c, &["rev-parse", "--verify", "HEAD"])?
            };
            let value = value.trim().to_owned();
            if !oid(&value) {
                return Err("Invalid subtree result; retained for inspection".into());
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
                    return Err("Split result tree differs from the selected committed folder; retained without publication consent".into());
                }
            }
            v.state.candidates[i].result = Some(value);
            v.state.candidates[i].stage = if c.kind == Kind::Split {
                Stage::SplitReady
            } else {
                Stage::Ready
            };
            v.state.candidates[i].notice =
                "Immutable result retained; original checkout and branch were not changed".into();
        }
        err => {
            v.state.candidates[i].stage = if !unmerged.is_empty() {
                Stage::Conflict
            } else {
                Stage::Paused
            };
            v.state.candidates[i].notice = format!(
                "{}\nUnmerged: {}\nCandidate status: {}\nInspect retained work. No original branch was updated and no helper is retried automatically.",
                git::redact(
                    &err.err()
                        .unwrap_or_else(|| "Helper left working changes".into()),
                    None
                ),
                unmerged,
                status
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
        return Err("Only an inspected retained import may be continued".into());
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
        return Err("Resolve and stage every candidate conflict before continuing; original files remain unchanged".into());
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
        Err("No acknowledged merge result exists. Inspect this candidate, then prepare a new update explicitly; the helper was not rerun".into())
    };
    finish(r, &mut view, i, result)?;
    Ok("Candidate inspected; refresh to review the immutable result".into())
}
fn accept_allowed(c: &Candidate, branch: &str, head: &str, clean: bool) -> Result<(), String> {
    if c.stage != Stage::Ready
        || c.kind == Kind::Split
        || c.result.is_none()
        || branch != c.branch
        || head != c.old
        || !clean
    {
        return Err("Acceptance requires the reviewed direct branch at its original commit and a completely clean original checkout".into());
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
        return Err("Review this exact candidate patch before accepting".into());
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
        return Err("Candidate changed; refresh and review it before accepting".into());
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
        c.result.as_deref().ok_or("No result")?,
    )?;
    view.state.candidates[i].stage = Stage::Accepted;
    view.state
        .sources
        .iter_mut()
        .find(|s| s.id == c.source)
        .ok_or("Source disappeared")?
        .imported = c.upstream;
    save(r.repo, &mut view)?;
    Ok("Subtree update accepted through the shared guarded checkout transaction; candidate and object pins retained".into())
}
pub(crate) fn reconcile(r: &Runner<'_>, v: &View, key: &str) -> Result<String, String> {
    let ctx = check(r.repo, v, false)?;
    let i = candidate(v, key)?;
    let c = &v.state.candidates[i];
    if c.stage != Stage::Accepting {
        return Err("Only an uncertain acceptance can be reconciled here; inspect ambiguous network sends at the remote before renewed approval".into());
    }
    clean_source(&ctx)?;
    let head = resolve(&ctx, "HEAD")?;
    let branch = branch(&ctx)?;
    let stage = if branch == c.branch && Some(head.as_str()) == c.result.as_deref() {
        Stage::Accepted
    } else if branch == c.branch && head == c.old {
        Stage::Ready
    } else {
        return Err(
            "Original checkout differs from both exact acceptance receipts; retained without retry"
                .into(),
        );
    };
    let mut view = v.clone();
    view.state.candidates[i].stage = stage;
    if stage == Stage::Accepted {
        view.state
            .sources
            .iter_mut()
            .find(|s| s.id == c.source)
            .ok_or("Source disappeared")?
            .imported = c.upstream.clone();
    }
    save(r.repo, &mut view)?;
    Ok("Exact original checkout receipt reconciled; no branch or working files changed".into())
}
pub(crate) fn close(r: &Runner<'_>, v: &View, key: &str) -> Result<String, String> {
    check(r.repo, v, false)?;
    let i = candidate(v, key)?;
    if matches!(
        v.state.candidates[i].stage,
        Stage::Accepting | Stage::Sending
    ) {
        return Err("Inspect/reconcile the uncertain outcome before closing its metadata".into());
    }
    let mut view = v.clone();
    view.state.candidates[i].stage = Stage::Closed;
    save(r.repo, &mut view)?;
    Ok(
        "Proposal closed; its worktree, results and pins remain retained for manual inspection"
            .into(),
    )
}
fn principal(c: Option<&git::Credentials>) -> Result<Option<(String, String, String)>, String> {
    c.map(|c| {
        let s = c
            .scope
            .as_ref()
            .ok_or("Sign-in lacks endpoint/account scope; sign in again")?;
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
            c.result.as_deref().ok_or("Split result missing")?,
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
        return Err("Split is not ready. Uncertain/rejected sends require explicit remote inspection and renewed approval".into());
    }
    let context = crate::connections::current();
    if context.error.is_some() || context.choices != profile.connections {
        return Err("Selected profile/connection changed; prepare again".into());
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
        return Err(
            "Publication preflight differs from the registered exact split destination".into(),
        );
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
        return Err("Prepared connection/account changed; prepare and approve again".into());
    }
    let i = candidate(&p.view, &p.id)?;
    let c = &p.view.state.candidates[i];
    if !publishable(c, p.renewed) {
        return Err("Split no longer eligible for this approval".into());
    }
    preflight::check(r.repo, &p.plan)?;
    let mut view = p.view.clone();
    view.state.candidates[i].stage = Stage::Sending;
    view.state.candidates[i].notice="Network dispatch intent persisted. Missing/rejected receipt never causes automatic retry. Inspect the remote before renewed approval".into();
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
    view.state.candidates[i].notice=result.as_ref().map(|_|"Exact split push acknowledged; local candidate/pins retained".into()).unwrap_or_else(|e|format!("Push rejected or uncertain: {}. Inspect destination before renewing approval; no automatic retry",git::redact(e,None)));
    save(r.repo, &mut view)?;
    result
}
#[cfg(test)]
mod tests;
#[cfg(all(test, unix))]
mod runtime_tests;
