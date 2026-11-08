//! Replays in persistent detached worktrees. Until explicit guarded acceptance, no
//! original branch, index or checkout is rewritten. Cancellation leaves inspectable work.
use super::*;
use crate::git::repository_tools::guard::Runner;
use std::path::PathBuf;
const REF: &str = "refs/gitcraft/deferred-rewrites/state";
#[derive(Clone, Serialize, Deserialize, Default)]
pub struct Jobs {
    pub jobs: Vec<Job>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: String,
    pub title: String,
    pub steps: Vec<Step>,
    pub cursor: usize,
    pub busy: bool,
    pub cancelled: bool,
    pub notice: String,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Step {
    pub branch: String,
    pub old: String,
    pub old_base: String,
    pub parent: String,
    pub onto: String,
    pub worktree: PathBuf,
    pub made: bool,
    #[serde(default)]
    pub root_identity: Option<String>,
    pub result: Option<String>,
    pub accepted: bool,
    pub conflicts: Vec<String>,
}
#[derive(Clone)]
pub struct View {
    pub jobs: Jobs,
    pub version: String,
    pub branch: String,
    pub branches: Vec<(String, String)>,
    common: PathBuf,
}
#[derive(Clone)]
pub struct Spec {
    pub expected: String,
    pub branch: String,
    pub old_base: String,
    pub onto: String,
    pub parent: String,
}
fn common(ctx: &Context, active: &dyn Fn() -> bool) -> Result<PathBuf, String> {
    if !cfg!(unix) {
        return Err("Deferred replay and acceptance require proven native Unix cwd ownership. No rewrite was dispatched on this platform".into());
    }
    if remote::is_remote(&ctx.repo) {
        return Err("Deferred rewrites require a native checkout and proven native acceptance. No remote rewrite was dispatched".into());
    }
    let bytes = ctx.source(
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        vec![],
        &[0],
        active,
    )?;
    let p = PathBuf::from(
        String::from_utf8(bytes)
            .map_err(|_| "Common directory must be UTF-8")?
            .trim_end_matches(['\r', '\n']),
    );
    if std::fs::canonicalize(&p).map_err(|e| e.to_string())? != p {
        return Err("Use the physical native repository path for deferred rewrites".into());
    }
    Ok(p)
}
fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() < 100 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}
fn validate(view: &View) -> Result<(), String> {
    if view.jobs.jobs.len() > 64 {
        return Err("At most 64 deferred jobs are supported".into());
    }
    let mut ids = BTreeSet::new();
    for job in &view.jobs.jobs {
        if !ids.insert(&job.id)
            || !valid_id(&job.id)
            || job.steps.is_empty()
            || job.steps.len() > 32
            || job.cursor > job.steps.len()
        {
            return Err("Invalid deferred job metadata".into());
        }
        let mut branches = BTreeSet::new();
        for (i, s) in job.steps.iter().enumerate() {
            let expected = view
                .common
                .join("gitcraft/deferred-rewrites")
                .join(&job.id)
                .join(format!("step-{i}"));
            if s.worktree != expected
                || s.branch.trim() != s.branch
                || s.branch.starts_with('-')
                || crate::git::notes::notes_ref(&s.branch).is_err()
                || !branches.insert(&s.branch)
                || (!s.parent.is_empty()
                    && (s.parent == s.branch || crate::git::notes::notes_ref(&s.parent).is_err()))
                || (s.made
                    && s.root_identity.as_ref().is_none_or(|v| {
                        v.is_empty()
                            || v.len() > 128
                            || !v.bytes().all(|b| b.is_ascii_digit() || b == b':')
                    }))
                || (!s.made && (s.result.is_some() || s.root_identity.is_some()))
                || (!s.parent.is_empty()
                    && job.steps[i..].iter().any(|later| later.branch == s.parent))
                || (s.accepted && s.result.is_none())
                || (i < job.cursor && s.result.is_none())
                || (i >= job.cursor && s.result.is_some())
                || !oid_valid(&s.old)
                || !oid_valid(&s.old_base)
                || !oid_valid(&s.onto)
                || s.result.as_ref().is_some_and(|r| !oid_valid(r))
            {
                return Err("Deferred worktree or immutable source metadata is invalid".into());
            }
        }
    }
    Ok(())
}
pub fn read(repo: &Path, active: &dyn Fn() -> bool) -> Result<View, String> {
    let ctx = Context::new(repo, active)?;
    let common = common(&ctx, active)?;
    if !ctx
        .source(&["symbolic-ref", "-q", REF], vec![], &[0, 1], active)?
        .is_empty()
    {
        return Err("Deferred registry must be a direct application ref".into());
    }
    let version = String::from_utf8(ctx.source(
        &["rev-parse", "--verify", "-q", REF],
        vec![],
        &[0, 1],
        active,
    )?)
    .map_err(|_| "Invalid deferred registry")?
    .trim()
    .to_owned();
    let jobs = if version.is_empty() {
        Jobs::default()
    } else {
        let n = ctx
            .text(&["cat-file", "-s", &version], vec![], active)?
            .parse::<usize>()
            .map_err(|_| "Invalid deferred registry size")?;
        if n > 2 * 1024 * 1024 {
            return Err("Deferred registry exceeds 2 MiB".into());
        }
        serde_json::from_slice(&ctx.run(&["cat-file", "blob", &version], vec![], &[0], active)?)
            .map_err(|_| "Invalid deferred registry JSON")?
    };
    let branch = String::from_utf8(ctx.source(
        &["symbolic-ref", "--short", "-q", "HEAD"],
        vec![],
        &[0, 1],
        active,
    )?)
    .map_err(|_| "Invalid current branch")?
    .trim()
    .to_owned();
    let refs = ctx.source(
        &[
            "for-each-ref",
            "--format=%(refname:short)%00%(objectname)",
            "refs/heads/",
        ],
        vec![],
        &[0],
        active,
    )?;
    if refs.len() > 2 * 1024 * 1024 {
        return Err("Branch preview exceeds 2 MiB".into());
    }
    let branches = std::str::from_utf8(&refs)
        .map_err(|_| "Invalid branches")?
        .lines()
        .filter_map(|s| {
            s.split_once('\0')
                .map(|(n, o)| (n.to_owned(), o.to_owned()))
        })
        .collect();
    let view = View {
        branches,
        jobs,
        version,
        branch,
        common,
    };
    validate(&view)?;
    Ok(view)
}
fn check(repo: &Path, v: &View, active: &dyn Fn() -> bool) -> Result<(), String> {
    let now = read(repo, active)?;
    if now.version != v.version || now.common != v.common {
        return Err("Deferred jobs changed; refresh before continuing".into());
    }
    Ok(())
}
fn save(repo: &Path, view: &mut View, active: &dyn Fn() -> bool) -> Result<(), String> {
    validate(view)?;
    let ctx = Context::new(repo, active)?;
    if common(&ctx, active)? != view.common {
        return Err("Repository identity changed".into());
    }
    let bytes = serde_json::to_vec(&view.jobs).map_err(|e| e.to_string())?;
    if bytes.len() > 2 * 1024 * 1024 {
        return Err("Deferred registry exceeds 2 MiB".into());
    }
    let new = ctx.blob(bytes, active)?;
    let zero = "0".repeat(new.len());
    let mut tx = format!(
        "start\noption no-deref\nupdate {REF} {new} {}\n",
        if view.version.is_empty() {
            &zero
        } else {
            &view.version
        }
    );
    // Immutable additive pins keep inputs and results reachable even if a worktree is
    // deliberately removed later. Published result pins are never overwritten or deleted.
    for job in &view.jobs.jobs {
        for s in &job.steps {
            for oid in [&s.old, &s.old_base, &s.onto]
                .into_iter()
                .chain(s.result.iter())
            {
                let pin = format!("refs/gitcraft/deferred-rewrites/pins/{}/{oid}", job.id);
                let previous = String::from_utf8(ctx.source(
                    &["rev-parse", "--verify", "-q", &pin],
                    vec![],
                    &[0, 1],
                    active,
                )?)
                .map_err(|_| "Invalid deferred pin")?
                .trim()
                .to_owned();
                if previous.is_empty() {
                    tx.push_str(&format!("create {pin} {oid}\n"));
                } else if previous != *oid {
                    return Err(
                        "An immutable deferred result pin changed; inspect it before continuing"
                            .into(),
                    );
                }
            }
        }
    }
    // The same OID can appear in several steps. Keep only one create command per pin.
    let mut seen = BTreeSet::new();
    let mut unique = String::new();
    for line in tx.lines() {
        if !line.starts_with("create ") || seen.insert(line.to_owned()) {
            unique.push_str(line);
            unique.push('\n');
        }
    }
    unique.push_str("prepare\ncommit\n");
    ctx.source(
        &[
            "update-ref",
            "-m",
            "gitcraft deferred rewrite state",
            "--stdin",
        ],
        unique.into_bytes(),
        &[0],
        active,
    )?;
    view.version = new;
    Ok(())
}
fn resolve(ctx: &Context, rev: &str, active: &dyn Fn() -> bool) -> Result<String, String> {
    if rev.is_empty() || rev.starts_with('-') || rev.chars().any(char::is_control) {
        return Err("Choose a branch or immutable commit".into());
    }
    String::from_utf8(ctx.source(
        &[
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{rev}^{{commit}}"),
        ],
        vec![],
        &[0],
        active,
    )?)
    .map(|s| s.trim().into())
    .map_err(|_| "Invalid revision".into())
}
fn safe_dir(path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        safe_dir(parent)?;
    }
    match std::fs::symlink_metadata(path) {
        Ok(m) if m.is_dir() && !m.file_type().is_symlink() => Ok(()),
        Ok(_) => Err("Deferred directory is linked or not a directory".into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let mut b = std::fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                b.mode(0o700);
            }
            b.create(path).map_err(|e| e.to_string())
        }
        Err(e) => Err(e.to_string()),
    }
}
// The fixed shell first enters and verifies the directory; Git inherits that cwd.
// Renaming/replacing its old pathname afterwards cannot redirect relative work-tree
// writes. Immutable argv supplies the previously checked source registration, so a
// replaced candidate .git file cannot redirect Git into another repository.
const OWNED_ALIAS: &str = r#"alias.gitcraft-owned-replay=!f() { pin=$1; path=$2; metadata=$3; shift 3; cd -P -- "$path" || exit 1; actual=$(LC_ALL=C stat -c '%d:%i' . 2>/dev/null || LC_ALL=C stat -f '%d:%i' .) || exit 1; [ "$actual" = "$pin" ] || exit 1; git --no-lazy-fetch --git-dir="$metadata" --work-tree=. "$@"; }; f"#;
fn owned_command(s: &Step, metadata: &Path, args: &[&str]) -> Result<Vec<String>, String> {
    if !cfg!(unix) {
        return Err("Deferred replay requires proven native Unix cwd ownership; no replay was dispatched on this platform".into());
    }
    let pin = s
        .root_identity
        .as_deref()
        .ok_or("No owned directory identity")?;
    Ok([
        "-c",
        OWNED_ALIAS,
        "gitcraft-owned-replay",
        pin,
        s.worktree.to_str().ok_or("Worktree path must be UTF-8")?,
        metadata.to_str().ok_or("Metadata path must be UTF-8")?,
    ]
    .into_iter()
    .chain(args.iter().copied())
    .map(str::to_owned)
    .collect())
}
fn owned_run(r: &Runner<'_>, common: &Path, s: &Step, args: &[&str]) -> Result<String, String> {
    owned(r.repo, common, s)?;
    let gitfile = crate::safe_read::read_inside(&s.worktree, ".git", 8192)
        .ok_or("Candidate Git link changed")?;
    let gd = std::str::from_utf8(&gitfile)
        .map_err(|_| "Invalid Git link")?
        .trim_end_matches(['\r', '\n'])
        .strip_prefix("gitdir: ")
        .ok_or("Candidate Git link changed")?;
    let words = owned_command(s, Path::new(gd), args)?;
    let result = run(
        r,
        r.repo,
        &words.iter().map(String::as_str).collect::<Vec<_>>(),
    );
    filesystem_owned(common, s)?;
    result
}
fn run(r: &Runner<'_>, path: &Path, args: &[&str]) -> Result<String, String> {
    let mut words = vec![
        "--no-lazy-fetch",
        "-c",
        "core.hooksPath=/dev/null",
        "-c",
        "core.fsmonitor=false",
        "-c",
        "commit.gpgSign=false",
        "-c",
        "rebase.updateRefs=false",
        "-c",
        "rebase.autoStash=false",
    ];
    words.extend(args);
    r.in_repo(path, &words)
}
pub fn start(
    r: &Runner<'_>,
    expected: &View,
    title: String,
    specs: Vec<Spec>,
) -> Result<String, String> {
    let active = || !r.control.is_cancelled();
    check(r.repo, expected, &active)?;
    if specs.is_empty() || specs.len() > 32 {
        return Err("Select 1–32 replay layers".into());
    }
    let ctx = Context::new(r.repo, &active)?;
    static IDS: AtomicU64 = AtomicU64::new(0);
    let id = format!(
        "{}-{}-{}",
        git::now(),
        std::process::id(),
        IDS.fetch_add(1, Ordering::Relaxed)
    );
    let mut branches = BTreeSet::new();
    let mut steps = Vec::new();
    for spec in specs {
        let reference = format!("refs/heads/{}", spec.branch);
        ctx.source(&["check-ref-format", &reference], vec![], &[0], &active)?;
        if !branches.insert(spec.branch.clone()) {
            return Err("A branch appears twice in the replay".into());
        }
        let old = resolve(&ctx, &reference, &active)?;
        if old != spec.expected
            || !ctx
                .source(
                    &["symbolic-ref", "-q", &reference],
                    vec![],
                    &[0, 1],
                    &active,
                )?
                .is_empty()
        {
            return Err(
                "Source branch changed or became indirect since preview; refresh before replaying"
                    .into(),
            );
        }

        let old_base = resolve(&ctx, &spec.old_base, &active)?;
        let onto = resolve(&ctx, &spec.onto, &active)?;
        ctx.source(
            &["merge-base", "--is-ancestor", &old_base, &old],
            vec![],
            &[0],
            &active,
        )
        .map_err(|_| "The old base must be an ancestor of the original branch")?;
        let worktree = expected
            .common
            .join("gitcraft/deferred-rewrites")
            .join(&id)
            .join(format!("step-{}", steps.len()));
        steps.push(Step {
            branch: spec.branch,
            old,
            old_base,
            parent: spec.parent,
            onto,
            worktree,
            made: false,
            root_identity: None,
            result: None,
            accepted: false,
            conflicts: Vec::new(),
        });
    }
    let mut view = expected.clone();
    view.jobs.jobs.push(Job {
        id: id.clone(),
        title,
        steps,
        cursor: 0,
        busy: false,
        cancelled: false,
        notice: "Prepared immutable replay inputs; original branches retained".into(),
    });
    save(r.repo, &mut view, &active)?;
    resume(r, &view, &id)
}
// Check filesystem provenance before asking Git to enter a retained candidate. A path
// and a registration alone do not prove that its .git still belongs to this repository.
fn ownership_paths(
    common: &Path,
    worktree: &Path,
    gitfile: &[u8],
    backlink: &[u8],
) -> Result<PathBuf, String> {
    let text = |b: &[u8]| {
        std::str::from_utf8(b)
            .ok()
            .map(|s| s.trim_end_matches(['\r', '\n']).to_owned())
    };
    let gitfile = text(gitfile).ok_or("Invalid owned Git link")?;
    let gitdir = PathBuf::from(
        gitfile
            .strip_prefix("gitdir: ")
            .ok_or("Candidate is not a linked worktree")?,
    );
    let relative = gitdir
        .strip_prefix(common.join("worktrees"))
        .map_err(|_| "Candidate belongs to another repository")?;
    if relative.components().count() != 1
        || !matches!(
            relative.components().next(),
            Some(std::path::Component::Normal(_))
        )
    {
        return Err("Candidate metadata is outside its owned registration".into());
    }
    let backlink = text(backlink).ok_or("Invalid owned Git backlink")?;
    if Path::new(&backlink) != worktree.join(".git") {
        return Err("Candidate registration backlink changed".into());
    }
    Ok(gitdir)
}
fn root_identity(path: &Path) -> Result<String, String> {
    #[cfg(unix)]
    let file = {
        use rustix::fs::{Mode, OFlags, open};
        std::fs::File::from(
            open(
                path,
                OFlags::RDONLY | OFlags::CLOEXEC | OFlags::DIRECTORY | OFlags::NOFOLLOW,
                Mode::empty(),
            )
            .map_err(|e| e.to_string())?,
        )
    };
    #[cfg(windows)]
    let file = {
        use std::os::windows::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .read(true)
            .share_mode(3)
            .custom_flags(0x0200_0000 | 0x0020_0000)
            .open(path)
            .map_err(|e| e.to_string())?
    };
    #[cfg(any(unix, windows))]
    return crate::git::recovery::native_file_identity(&file).map(|v| v.0);
    #[cfg(not(any(unix, windows)))]
    Err("Stable worktree directory identities are unavailable on this platform".into())
}
fn filesystem_owned(common: &Path, s: &Step) -> Result<(), String> {
    if std::fs::canonicalize(&s.worktree).map_err(|e| e.to_string())? != s.worktree
        || s.root_identity.as_deref() != Some(root_identity(&s.worktree)?.as_str())
    {
        return Err("Deferred worktree directory identity/path changed".into());
    }
    let gitfile = crate::safe_read::read_inside(&s.worktree, ".git", 8192)
        .ok_or("Candidate Git link is missing, linked or oversized")?;
    let raw = std::str::from_utf8(&gitfile).map_err(|_| "Invalid owned Git link")?;
    let gitdir = PathBuf::from(
        raw.trim_end_matches(['\r', '\n'])
            .strip_prefix("gitdir: ")
            .ok_or("Candidate is not a linked worktree")?,
    );
    let relative = gitdir
        .strip_prefix(common)
        .map_err(|_| "Candidate belongs to another repository")?;
    let relative = relative.to_str().ok_or("Invalid candidate metadata path")?;
    let backlink = crate::safe_read::read_inside(common, &format!("{relative}/gitdir"), 8192)
        .ok_or("Candidate backlink is missing or linked")?;
    let metadata = ownership_paths(common, &s.worktree, &gitfile, &backlink)?;
    if std::fs::canonicalize(&metadata).map_err(|e| e.to_string())? != metadata {
        return Err("Candidate registration identity changed".into());
    }
    let commondir = crate::safe_read::read_inside(common, &format!("{relative}/commondir"), 8192)
        .ok_or("Candidate common directory link is missing")?;
    let common_text = std::str::from_utf8(&commondir)
        .map_err(|_| "Invalid candidate common directory")?
        .trim_end_matches(['\r', '\n']);
    if std::fs::canonicalize(metadata.join(common_text)).map_err(|e| e.to_string())? != common {
        return Err("Candidate common directory changed".into());
    }
    Ok(())
}
fn owned(repo: &Path, common: &Path, s: &Step) -> Result<(), String> {
    filesystem_owned(common, s)?;
    let wt = git::worktree_list(repo, None)?
        .into_iter()
        .find(|w| w.path == s.worktree)
        .ok_or("Deferred worktree is missing or no longer registered; no replay was dispatched")?;
    if wt.branch.is_some() {
        return Err("Deferred replay worktree is attached to a branch; detach it before continuing so no original branch is rewritten".into());
    }
    // Repeat the cheap filesystem check after reading registrations.
    filesystem_owned(common, s)
}
fn acceptance_allowed(job: &Job, undo: bool) -> Result<(), String> {
    if job.busy {
        return Err("Acceptance outcome is uncertain. Inspect its recovery evidence and reconcile exact receipts before any acceptance or undo retry".into());
    }
    if job.cursor != job.steps.len() || (job.cancelled && !undo) {
        return Err("Complete every candidate before accepting; closed proposals can only undo already accepted layers".into());
    }
    Ok(())
}
#[derive(Debug, PartialEq, Eq)]
enum Replay {
    Continue,
    Reconcile,
    Start,
}
fn replay(pending: bool, head_is_old: bool) -> Replay {
    if pending {
        Replay::Continue
    } else if head_is_old {
        Replay::Start
    } else {
        Replay::Reconcile
    }
}
fn rebasing(path: &Path) -> Result<bool, String> {
    for name in ["rebase-merge", "rebase-apply"] {
        let p = git::run(
            Some(path),
            &["rev-parse", "--path-format=absolute", "--git-path", name],
        )?;
        if std::fs::symlink_metadata(p.trim()).is_ok() {
            return Ok(true);
        }
    }
    Ok(false)
}
fn conflicts(path: &Path) -> Result<Vec<String>, String> {
    let raw = git::run(
        Some(path),
        &[
            "diff",
            "--name-only",
            "--diff-filter=U",
            "-z",
            "--no-ext-diff",
            "--no-textconv",
            "--",
        ],
    )?;
    let paths = untracked_paths(raw.into_bytes())?;
    Ok(paths.into_iter().collect())
}
pub fn resume(r: &Runner<'_>, expected: &View, id: &str) -> Result<String, String> {
    let active = || !r.control.is_cancelled();
    check(r.repo, expected, &active)?;
    let mut view = expected.clone();
    let j = view
        .jobs
        .jobs
        .iter()
        .position(|j| j.id == id)
        .ok_or("Job disappeared")?;
    if view.jobs.jobs[j].cancelled {
        return Err("This proposal is closed; its worktrees and results were retained".into());
    }
    while view.jobs.jobs[j].cursor < view.jobs.jobs[j].steps.len() {
        let i = view.jobs.jobs[j].cursor;
        if i > 0 {
            let parent = view.jobs.jobs[j].steps[i].parent.clone();
            if let Some(onto) = view.jobs.jobs[j].steps[..i]
                .iter()
                .find(|s| s.branch == parent)
                .and_then(|s| s.result.clone())
            {
                view.jobs.jobs[j].steps[i].onto = onto;
            }
        }
        view.jobs.jobs[j].busy = true;
        view.jobs.jobs[j].notice="Replay is in flight or its outcome needs reconciliation after interruption. Continue explicitly to inspect its owned worktree".into();
        save(r.repo, &mut view, &active)?;
        let mut s = view.jobs.jobs[j].steps[i].clone();
        if !s.made {
            safe_dir(
                s.worktree
                    .parent()
                    .ok_or("Missing owned workspace parent")?,
            )?;
            if s.worktree.exists() {
                return Err(
                    "Owned path already exists; it was retained rather than adopted or reset"
                        .into(),
                );
            }
            let result = run(
                r,
                r.repo,
                &[
                    "worktree",
                    "add",
                    "--detach",
                    "--lock",
                    "--reason=gitcraft deferred replay",
                    s.worktree.to_str().ok_or("Worktree path must be UTF-8")?,
                    &s.old,
                ],
            );
            if let Err(e) = result {
                view.jobs.jobs[j].notice = format!(
                    "Worktree creation stopped; inspect the retained path before retrying: {e}"
                );
                save(r.repo, &mut view, &active)?;
                return Err(e);
            }
            s.root_identity = Some(root_identity(&s.worktree)?);
            s.made = true;
            view.jobs.jobs[j].steps[i] = s.clone();
            save(r.repo, &mut view, &active)?;
        }
        owned(r.repo, &view.common, &s)?;
        let pending = rebasing(&s.worktree)?;
        let head = git::resolve(&s.worktree, "HEAD")?;
        let result = match replay(pending, head == s.old) {
            Replay::Continue => owned_run(
                r,
                &view.common,
                &s,
                &["-c", "core.editor=true", "rebase", "--continue"],
            ),
            Replay::Reconcile => {
                // Explicit continuation reconciles a completed detached replay after a lost
                // acknowledgement. It does not rerun or reset that user's resolution work.
                Ok("Reconciled the current detached candidate".into())
            }
            Replay::Start => owned_run(
                r,
                &view.common,
                &s,
                &[
                    "rebase",
                    "--no-update-refs",
                    "--no-autostash",
                    "--rebase-merges",
                    "--onto",
                    &s.onto,
                    &s.old_base,
                ],
            ),
        };
        let remaining = conflicts(&s.worktree)?;
        view.jobs.jobs[j].steps[i].conflicts = remaining.clone();
        if result.is_err() || !remaining.is_empty() || rebasing(&s.worktree)? {
            view.jobs.jobs[j].busy = false;
            view.jobs.jobs[j].notice = if remaining.is_empty() {
                "Replay stopped. Inspect the owned worktree and continue explicitly; originals were retained".into()
            } else {
                format!(
                    "Conflicts waiting in {}. Resolve and stage them there, then Continue",
                    s.branch
                )
            };
            save(r.repo, &mut view, &active)?;
            return result
                .map(|_| view.jobs.jobs[j].notice.clone())
                .or_else(|_| Ok(view.jobs.jobs[j].notice.clone()));
        }
        if !git::run(
            Some(&s.worktree),
            &["status", "--porcelain=v1", "--untracked-files=all"],
        )?
        .is_empty()
        {
            return Err("Candidate has uncommitted/untracked work; retain or commit it before declaring the replay complete".into());
        }
        let candidate = git::resolve(&s.worktree, "HEAD")?;
        git::run(
            Some(&s.worktree),
            &["merge-base", "--is-ancestor", &s.onto, &candidate],
        )
        .map_err(
            |_| "Candidate no longer contains its reviewed new base; inspect it before continuing",
        )?;
        view.jobs.jobs[j].steps[i].result = Some(candidate);
        view.jobs.jobs[j].cursor += 1;
        view.jobs.jobs[j].busy = false;
        view.jobs.jobs[j].notice = "Candidate layer saved; original branch retained".into();
        save(r.repo, &mut view, &active)?;
    }
    Ok(
        "Every candidate is ready for explicit guarded acceptance. Original branches are unchanged"
            .into(),
    )
}
pub fn close(
    repo: &Path,
    expected: &View,
    id: &str,
    active: &dyn Fn() -> bool,
) -> Result<String, String> {
    check(repo, expected, active)?;
    let mut next = expected.clone();
    let j = next
        .jobs
        .jobs
        .iter_mut()
        .find(|j| j.id == id)
        .ok_or("Job disappeared")?;
    j.cancelled = true;
    j.notice =
        "Proposal closed; owned worktrees, resolution work and immutable results retained".into();
    save(repo, &mut next, active)?;
    Ok("Proposal closed; nothing was deleted or reset".into())
}
fn acceptance_place(
    r: &Runner<'_>,
    view: &View,
    id: &str,
    i: usize,
    branch: &str,
) -> Result<PathBuf, String> {
    let worktrees = git::worktree_list(r.repo, None)?;
    let places = worktrees
        .into_iter()
        .filter(|w| w.branch.as_deref() == Some(branch))
        .collect::<Vec<_>>();
    if places.len() > 1 {
        return Err("Branch is checked out in multiple worktrees; make its acceptance target unambiguous first".into());
    }
    if let Some(w) = places.first() {
        return Ok(w.path.clone());
    }
    let p = view
        .common
        .join("gitcraft/deferred-rewrites")
        .join(id)
        .join(format!("accept-{i}"));
    if p.exists() {
        return Err("An earlier acceptance workspace exists but is not attached to this branch; inspect it instead of overwriting it".into());
    }
    safe_dir(p.parent().ok_or("Missing acceptance parent")?)?;
    run(
        r,
        r.repo,
        &[
            "worktree",
            "add",
            "--lock",
            "--reason=gitcraft guarded acceptance",
            p.to_str().ok_or("Invalid acceptance path")?,
            branch,
        ],
    )?;
    Ok(p)
}
pub fn accept(r: &Runner<'_>, expected: &View, id: &str, undo: bool) -> Result<String, String> {
    let active = || !r.control.is_cancelled();
    check(r.repo, expected, &active)?;
    let mut view = expected.clone();
    let j = view
        .jobs
        .jobs
        .iter()
        .position(|j| j.id == id)
        .ok_or("Job disappeared")?;
    acceptance_allowed(&view.jobs.jobs[j], undo)?;
    let indices = if undo {
        (0..view.jobs.jobs[j].steps.len()).rev().collect::<Vec<_>>()
    } else {
        (0..view.jobs.jobs[j].steps.len()).collect()
    };
    for i in indices {
        let s = view.jobs.jobs[j].steps[i].clone();
        if s.accepted != undo {
            continue;
        }
        let result = s.result.as_deref().ok_or("No immutable candidate result")?;
        let (old, new) = if undo {
            (result, s.old.as_str())
        } else {
            (s.old.as_str(), result)
        };
        if git::resolve(r.repo, &format!("refs/heads/{}", s.branch))? != old {
            return Err(format!(
                "{} changed since this proposal; no update of that branch was attempted",
                s.branch
            ));
        }
        owned(r.repo, &view.common, &s)?;
        if !undo
            && (git::resolve(&s.worktree, "HEAD")? != result
                || rebasing(&s.worktree)?
                || !git::run(
                    Some(&s.worktree),
                    &["status", "--porcelain=v1", "--untracked-files=all"],
                )?
                .is_empty())
        {
            return Err("Candidate changed or contains unresolved work; refresh and review it before accepting".into());
        }
        let place = acceptance_place(r, &view, id, i, &s.branch)?;
        view.jobs.jobs[j].busy = true;
        view.jobs.jobs[j].notice = format!(
            "Guarded {} of {} in flight. An interrupted acknowledgement requires inspection before retrying",
            if undo { "undo" } else { "acceptance" },
            s.branch
        );
        save(r.repo, &mut view, &active)?;
        let result = crate::push_preflight::accept_rewrite(
            &place,
            &format!("refs/heads/{}", s.branch),
            old,
            new,
        );
        if result.is_ok() {
            view.jobs.jobs[j].steps[i].accepted = !undo;
            view.jobs.jobs[j].busy = false;
            view.jobs.jobs[j].notice = format!(
                "{} {} as {}",
                if undo { "Restored" } else { "Accepted" },
                s.branch,
                new
            );
            save(r.repo, &mut view, &active)?;
        } else {
            view.jobs.jobs[j].notice="Guarded acceptance stopped. Earlier successful layers retain their receipts. Inspect HEAD, index, files and the native recovery evidence before retrying; no automatic reset was attempted".into();
            save(r.repo, &mut view, &active)?;
            return result.map(|_| String::new());
        }
    }
    Ok(if undo {
        "Restored accepted original tips through guarded transactions"
    } else {
        "Accepted candidate layers through guarded transactions; per-layer receipts retained"
    }
    .into())
}

pub fn reconcile(
    repo: &Path,
    expected: &View,
    id: &str,
    active: &dyn Fn() -> bool,
) -> Result<String, String> {
    check(repo, expected, active)?;
    let mut next = expected.clone();
    let j = next
        .jobs
        .jobs
        .iter_mut()
        .find(|j| j.id == id)
        .ok_or("Job disappeared")?;
    for s in &mut j.steps {
        let Some(candidate) = &s.result else {
            continue;
        };
        let actual = git::resolve(repo, &format!("refs/heads/{}", s.branch))?;
        if actual != s.old && actual != *candidate {
            return Err("An original branch has another tip; preserve and review that new work instead of reconciling automatically".into());
        }
        for w in git::worktree_list(repo, None)?
            .iter()
            .filter(|w| w.branch.as_deref() == Some(&s.branch))
        {
            if !git::run(
                Some(&w.path),
                &["status", "--porcelain=v1", "--untracked-files=all"],
            )?
            .is_empty()
            {
                return Err("An original worktree has edits or an incomplete checkout. Inspect and resolve it before reconciling the acceptance receipt".into());
            }
        }
        if s.old != *candidate {
            s.accepted = actual == *candidate;
        }
    }
    j.busy = false;
    j.notice="Acceptance receipts reconciled to exact original/candidate OIDs after clean worktree inspection".into();
    save(repo, &mut next, active)?;
    Ok("Reconciled exact branch tips; no files or branches were changed".into())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    pub(crate) fn view() -> View {
        let common = PathBuf::from("/private/synthetic/project/.git");
        let oid = "a".repeat(40);
        View {
            jobs: Jobs {
                jobs: vec![Job {
                    id: "synthetic-proposal".into(),
                    title: "feature after main".into(),
                    cursor: 0,
                    busy: false,
                    cancelled: false,
                    notice: "Conflict waiting in isolated candidate".into(),
                    steps: vec![Step {
                        branch: "feature".into(),
                        old: oid.clone(),
                        old_base: "b".repeat(40),
                        parent: "main".into(),
                        onto: "c".repeat(40),
                        worktree: common
                            .join("gitcraft/deferred-rewrites/synthetic-proposal/step-0"),
                        made: true,
                        root_identity: Some("1:32".into()),
                        result: None,
                        accepted: false,
                        conflicts: vec!["src/shared.rs".into()],
                    }],
                }],
            },
            version: "d".repeat(40),
            branch: "feature".into(),
            branches: vec![("feature".into(), oid)],
            common,
        }
    }
    #[test]
    fn pure_deferred_ownership_requires_exact_repository_registration_and_backlink() {
        let v = view();
        let s = &v.jobs.jobs[0].steps[0];
        let gd = v.common.join("worktrees/step-0");
        let gitfile = format!("gitdir: {}\n", gd.display());
        let backlink = format!("{}\n", s.worktree.join(".git").display());
        assert_eq!(
            ownership_paths(
                &v.common,
                &s.worktree,
                gitfile.as_bytes(),
                backlink.as_bytes()
            )
            .unwrap(),
            gd
        );
        for foreign in [
            "gitdir: /other/.git/worktrees/step-0\n",
            "gitdir: /private/synthetic/project/.git/worktrees/../other\n",
            "gitdir: /private/synthetic/project/.git/worktrees/step-0/other\n",
            "ref: refs/heads/main\n",
        ] {
            assert!(
                ownership_paths(
                    &v.common,
                    &s.worktree,
                    foreign.as_bytes(),
                    backlink.as_bytes()
                )
                .is_err()
            );
        }
        assert!(
            ownership_paths(&v.common, &s.worktree, gitfile.as_bytes(), b"/other/.git\n").is_err()
        );
    }
    #[test]
    fn pure_deferred_metadata_refuses_unowned_paths_receipt_gaps_duplicate_layers_and_forward_parents()
     {
        let original = view();
        assert!(validate(&original).is_ok());
        let mut v = original.clone();
        v.jobs.jobs[0].steps[0].worktree = "/unrelated".into();
        assert!(validate(&v).is_err());
        let mut v = original.clone();
        v.jobs.jobs[0].steps[0].root_identity = None;
        assert!(validate(&v).is_err());
        let mut v = original.clone();
        v.jobs.jobs[0].cursor = 1;
        assert!(validate(&v).is_err());
        let mut v = original.clone();
        v.jobs.jobs[0].steps[0].accepted = true;
        assert!(validate(&v).is_err());
        let mut v = original.clone();
        v.jobs.jobs.push(v.jobs.jobs[0].clone());
        assert!(validate(&v).is_err());
        let mut v = original.clone();
        let mut step = v.jobs.jobs[0].steps[0].clone();
        step.worktree = v
            .common
            .join("gitcraft/deferred-rewrites/synthetic-proposal/step-1");
        v.jobs.jobs[0].steps.push(step);
        assert!(validate(&v).is_err());
        let mut v = original.clone();
        v.jobs.jobs[0].steps[0].parent = "feature".into();
        assert!(validate(&v).is_err());
        let mut v = original.clone();
        v.jobs.jobs[0].steps[0].branch = " feature ".into();
        assert!(validate(&v).is_err());
        let mut v = original;
        let mut step = v.jobs.jobs[0].steps[0].clone();
        step.branch = "main".into();
        step.parent = "upstream".into();
        step.worktree = v
            .common
            .join("gitcraft/deferred-rewrites/synthetic-proposal/step-1");
        v.jobs.jobs[0].steps.push(step);
        assert!(validate(&v).is_err());
    }
    #[test]
    fn pure_deferred_replay_never_resets_resolutions_and_uncertain_acceptance_requires_inspection()
    {
        assert_eq!(replay(true, true), Replay::Continue);
        assert_eq!(replay(true, false), Replay::Continue);
        assert_eq!(replay(false, true), Replay::Start);
        assert_eq!(replay(false, false), Replay::Reconcile);
        let mut j = view().jobs.jobs.remove(0);
        assert!(acceptance_allowed(&j, false).is_err());
        j.cursor = 1;
        j.steps[0].result = Some("e".repeat(40));
        assert!(acceptance_allowed(&j, false).is_ok());
        j.busy = true;
        assert!(acceptance_allowed(&j, false).is_err());
        assert!(acceptance_allowed(&j, true).is_err());
        j.busy = false;
        j.cancelled = true;
        j.steps[0].accepted = true;
        assert!(acceptance_allowed(&j, false).is_err());
        assert!(acceptance_allowed(&j, true).is_ok());
    }
    #[test]
    fn pure_deferred_recording_enters_verified_cwd_before_git_and_keeps_paths_as_argv() {
        let mut v = view();
        let s = &mut v.jobs.jobs[0].steps[0];
        s.worktree = PathBuf::from("/private/synthetic/space ; $(not-executed)");
        let metadata = Path::new("/private/synthetic/.git/worktrees/step-0");
        let command = owned_command(s, metadata, &["rebase", "--continue"]).unwrap();
        assert_eq!(command[3], "1:32");
        assert_eq!(command[4], s.worktree.to_str().unwrap());
        assert_eq!(&command[6..], ["rebase", "--continue"]);
        assert!(!command[1].contains("not-executed"));
        assert!(OWNED_ALIAS.find("cd -P").unwrap() < OWNED_ALIAS.find("stat -c").unwrap());
        assert!(
            OWNED_ALIAS.find(r#"[ "$actual" = "$pin" ]"#).unwrap()
                < OWNED_ALIAS.find("git --no-lazy-fetch --git-dir=").unwrap()
        );
        assert!(OWNED_ALIAS.contains("--work-tree=."));
        assert!(!OWNED_ALIAS.contains("eval"));
    }
}

#[cfg(test)]
#[path = "deferred/runtime_tests.rs"]
mod runtime_tests;
