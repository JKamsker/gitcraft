//! Subtree registry validation, persistence, and registration.

use super::*;

pub(super) fn validate(r: &Registry, common: &Path) -> Result<(), String> {
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

pub(super) fn source<'a>(v: &'a View, key: &str) -> Result<&'a Source, String> {
    v.state
        .sources
        .iter()
        .find(|s| s.id == key)
        .ok_or(crate::i18n::text("backend-registered-subtree-disappeared-359d79").into())
}

pub(super) fn candidate(v: &View, key: &str) -> Result<usize, String> {
    v.state
        .candidates
        .iter()
        .position(|c| c.id == key)
        .ok_or(crate::i18n::text("backend-subtree-candidate-disappeared-a8d155").into())
}

pub(super) fn bytes(ctx: &Context, args: &[&str], ok: &[i32]) -> Result<Vec<u8>, String> {
    ctx.source(args, vec![], ok, &|| {
        !crate::tasks::cancellation().is_cancelled()
    })
}

pub(super) fn text(ctx: &Context, args: &[&str], ok: &[i32]) -> Result<String, String> {
    String::from_utf8(bytes(ctx, args, ok)?)
        .map(|s| s.trim_end_matches(['\r', '\n']).to_owned())
        .map_err(|_| crate::i18n::text("backend-subtree-metadata-requires-utf-8-2132f4").into())
}

pub(super) fn common(ctx: &Context) -> Result<PathBuf, String> {
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

pub(super) fn state(ctx: &Context) -> Result<(String, Registry), String> {
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

pub(super) fn check(repo: &Path, v: &View, head: bool) -> Result<Context, String> {
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

pub(super) fn save(repo: &Path, v: &mut View) -> Result<(), String> {
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

pub(super) fn next_id() -> String {
    static IDS: AtomicU64 = AtomicU64::new(0);
    format!(
        "{}-{}-{}",
        git::now(),
        std::process::id(),
        IDS.fetch_add(1, Ordering::Relaxed)
    )
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
