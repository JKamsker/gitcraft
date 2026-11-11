//! Deferred replay journal validation, persistence, and repository binding.

use super::*;

pub(super) fn common(ctx: &Context, active: &dyn Fn() -> bool) -> Result<PathBuf, String> {
    if !cfg!(unix) {
        return Err(crate::i18n::text(
            "backend-deferred-replay-and-acceptance-require-proven-native-unix-cwd-own-d64834",
        )
        .into());
    }
    if remote::is_remote(&ctx.repo) {
        return Err(crate::i18n::text(
            "backend-deferred-rewrites-require-a-native-checkout-and-proven-native-acc-dce663",
        )
        .into());
    }
    let bytes = ctx.source(
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        vec![],
        &[0],
        active,
    )?;
    let p = PathBuf::from(
        String::from_utf8(bytes)
            .map_err(|_| crate::i18n::text("backend-common-directory-must-be-utf-8-6f8d3c"))?
            .trim_end_matches(['\r', '\n']),
    );
    if std::fs::canonicalize(&p).map_err(|e| e.to_string())? != p {
        return Err(crate::i18n::text(
            "backend-use-the-physical-native-repository-path-for-deferred-rewrites-ac2265",
        )
        .into());
    }
    Ok(p)
}

pub(super) fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() < 100 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

pub(super) fn validate(view: &View) -> Result<(), String> {
    if view.jobs.jobs.len() > 64 {
        return Err(
            crate::i18n::text("backend-at-most-64-deferred-jobs-are-supported-a36bd5").into(),
        );
    }
    let mut ids = BTreeSet::new();
    for job in &view.jobs.jobs {
        if !ids.insert(&job.id)
            || !valid_id(&job.id)
            || job.steps.is_empty()
            || job.steps.len() > 32
            || job.cursor > job.steps.len()
        {
            return Err(crate::i18n::text("backend-invalid-deferred-job-metadata-cd6ec7").into());
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
                return Err(crate::i18n::text(
                    "backend-deferred-worktree-or-immutable-source-metadata-is-invalid-49791b",
                )
                .into());
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
        return Err(crate::i18n::text(
            "backend-deferred-registry-must-be-a-direct-application-ref-4533c7",
        )
        .into());
    }
    let version = String::from_utf8(ctx.source(
        &["rev-parse", "--verify", "-q", REF],
        vec![],
        &[0, 1],
        active,
    )?)
    .map_err(|_| crate::i18n::text("backend-invalid-deferred-registry-501b3e"))?
    .trim()
    .to_owned();
    let jobs = if version.is_empty() {
        Jobs::default()
    } else {
        let n = ctx
            .text(&["cat-file", "-s", &version], vec![], active)?
            .parse::<usize>()
            .map_err(|_| crate::i18n::text("backend-invalid-deferred-registry-size-465b87"))?;
        if n > 2 * 1024 * 1024 {
            return Err(crate::i18n::text("backend-deferred-registry-exceeds-2-mib-7f5238").into());
        }
        serde_json::from_slice(&ctx.run(&["cat-file", "blob", &version], vec![], &[0], active)?)
            .map_err(|_| crate::i18n::text("backend-invalid-deferred-registry-json-8fc61b"))?
    };
    let branch = String::from_utf8(ctx.source(
        &["symbolic-ref", "--short", "-q", "HEAD"],
        vec![],
        &[0, 1],
        active,
    )?)
    .map_err(|_| crate::i18n::text("backend-invalid-current-branch-34f949"))?
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
        return Err(crate::i18n::text("backend-branch-preview-exceeds-2-mib-c90983").into());
    }
    let branches = std::str::from_utf8(&refs)
        .map_err(|_| crate::i18n::text("backend-invalid-branches-683248"))?
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

pub(super) fn check(repo: &Path, v: &View, active: &dyn Fn() -> bool) -> Result<(), String> {
    let now = read(repo, active)?;
    if now.version != v.version || now.common != v.common {
        return Err(crate::i18n::text(
            "backend-deferred-jobs-changed-refresh-before-continuing-0f9e9d",
        )
        .into());
    }
    Ok(())
}

pub(super) fn save(repo: &Path, view: &mut View, active: &dyn Fn() -> bool) -> Result<(), String> {
    validate(view)?;
    let ctx = Context::new(repo, active)?;
    if common(&ctx, active)? != view.common {
        return Err(crate::i18n::text("backend-repository-identity-changed-2f0e64").into());
    }
    let bytes = serde_json::to_vec(&view.jobs).map_err(|e| e.to_string())?;
    if bytes.len() > 2 * 1024 * 1024 {
        return Err(crate::i18n::text("backend-deferred-registry-exceeds-2-mib-7f5238").into());
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
                .map_err(|_| crate::i18n::text("backend-invalid-deferred-pin-e1fc9c"))?
                .trim()
                .to_owned();
                if previous.is_empty() {
                    tx.push_str(&format!("create {pin} {oid}\n"));
                } else if previous != *oid {
                    return Err(
                        crate::i18n::text("backend-an-immutable-deferred-result-pin-changed-inspect-it-before-contin-1175c3")
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

pub(super) fn resolve(
    ctx: &Context,
    rev: &str,
    active: &dyn Fn() -> bool,
) -> Result<String, String> {
    if rev.is_empty() || rev.starts_with('-') || rev.chars().any(char::is_control) {
        return Err(crate::i18n::text("backend-choose-a-branch-or-immutable-commit-360acf").into());
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
    .map_err(|_| crate::i18n::text("backend-invalid-revision-3e52aa").into())
}

pub(super) fn safe_dir(path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        safe_dir(parent)?;
    }
    match std::fs::symlink_metadata(path) {
        Ok(m) if m.is_dir() && !m.file_type().is_symlink() => Ok(()),
        Ok(_) => Err(crate::i18n::text(
            "backend-deferred-directory-is-linked-or-not-a-directory-925871",
        )
        .into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let b = std::fs::DirBuilder::new();
            #[cfg(unix)]
            let b = {
                use std::os::unix::fs::DirBuilderExt;
                let mut b = b;
                b.mode(0o700);
                b
            };
            b.create(path).map_err(|e| e.to_string())
        }
        Err(e) => Err(e.to_string()),
    }
}
