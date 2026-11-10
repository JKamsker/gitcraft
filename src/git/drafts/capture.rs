//! Draft capture and source assembly without changing the real index.

use super::*;

pub fn capture(
    repo: &Path,
    reason: &str,
    active: &dyn Fn() -> bool,
) -> Result<Option<Draft>, String> {
    capture_preserving(repo, reason, None, None, active)
}

pub(super) fn capture_preserving(
    repo: &Path,
    reason: &str,
    keep: Option<&Id>,
    force_path: Option<&str>,
    requested: &dyn Fn() -> bool,
) -> Result<Option<Draft>, String> {
    let paths = force_path.map(|p| vec![p.to_owned()]).unwrap_or_default();
    capture_preserving_paths(repo, reason, keep, &paths, requested)
}

pub(super) fn capture_preserving_paths(
    repo: &Path,
    reason: &str,
    keep: Option<&Id>,
    force_paths: &[String],
    requested: &dyn Fn() -> bool,
) -> Result<Option<Draft>, String> {
    let start = std::time::Instant::now();
    let active = || requested() && start.elapsed() < std::time::Duration::from_secs(120);
    let ctx = Context::new(repo, &active)?;
    let before = source(&ctx, &active)?;
    let index_tree = tree(&ctx, &before.index, &active)?;
    cached_index(&ctx, &before, &active)?;
    let initial_changes = changed(&ctx, &before, &index_tree, &active)?;
    let mut candidates = initial_changes.clone();
    let untracked = untracked_paths(ctx.source(
        &["ls-files", "--others", "--exclude-standard", "-z"],
        vec![],
        &[0],
        &active,
    )?)?;
    candidates.extend(untracked.iter().cloned());
    for path in force_paths {
        valid_path(path)?;
        candidates.insert(path.to_owned());
    }
    // Materialized sparse entries can have edits hidden by skip-worktree.
    for path in &before.skip {
        if read_file(&ctx, path, &before.index[path].mode, &active)?.is_some() {
            candidates.insert(path.clone());
        }
    }
    if candidates.len() > MAX_FILES {
        return Err(crate::i18n::text(
            "backend-draft-has-over-2000-changed-untracked-files-make-a-manual-backup-b502fa",
        )
        .into());
    }
    if candidates.is_empty() {
        cleanup_in(
            &ctx,
            &list_in(&ctx, &active)?,
            git::now(),
            Retention::default(),
            keep,
            &active,
        )?;
        return Ok(None);
    }
    let mut work = before.index.clone();
    let mut files = Vec::new();
    let mut observed = BTreeMap::new();
    let mut total = 0;
    for path in candidates {
        let index = before.index.get(&path).cloned();
        if index.as_ref().is_some_and(|e| e.mode == "160000") {
            return Err(crate::i18n::text(
                "backend-drafts-exclude-submodule-contents-back-up-submodule-changes-separ-a309a6",
            )
            .into());
        }
        let data = read_file(
            &ctx,
            &path,
            index.as_ref().map_or("100644", |e| e.mode.as_str()),
            &active,
        )?;
        let entry = if let Some((mode, bytes)) = &data {
            total += bytes.len();
            if total > TOTAL_LIMIT {
                return Err(
                    crate::i18n::text("backend-draft-content-exceeds-32-mib-make-a-manual-backup-before-continui-5603e0").into(),
                );
            }
            Some(Entry {
                mode: mode.clone(),
                oid: ctx.blob(bytes.clone(), &active)?,
            })
        } else {
            None
        };
        match &entry {
            Some(e) => {
                work.insert(path.clone(), e.clone());
            }
            None => {
                work.remove(&path);
            }
        }
        files.push(File {
            path: path.clone(),
            index,
            work: entry,
            untracked: untracked.contains(&path),
        });
        observed.insert(path, data);
    }
    let work_tree = tree(&ctx, &work, &active)?;
    if source(&ctx, &active)? != before {
        return Err(crate::i18n::text(
            "backend-head-or-index-changed-while-making-a-draft-it-will-retry-after-a-c7f805",
        )
        .into());
    }
    tree(&ctx, &before.index, &active)?;
    cached_index(&ctx, &before, &active)?;
    if changed(&ctx, &before, &index_tree, &active)? != initial_changes
        || untracked_paths(ctx.source(
            &["ls-files", "--others", "--exclude-standard", "-z"],
            vec![],
            &[0],
            &active,
        )?)? != untracked
    {
        return Err(crate::i18n::text(
            "backend-working-changes-moved-while-making-a-draft-it-will-retry-after-a-1f8fd4",
        )
        .into());
    }
    for (path, expected) in observed {
        if read_file(
            &ctx,
            &path,
            before
                .index
                .get(&path)
                .map_or("100644", |e| e.mode.as_str()),
            &active,
        )? != expected
        {
            return Err(crate::i18n::text(
                "backend-a-working-file-changed-while-making-a-draft-it-will-retry-after-a-3a7281",
            )
            .into());
        }
    }
    let existing = list_in(&ctx, &active)?;
    if let Some(last) = existing.first().filter(|d| {
        git::now().saturating_sub(d.metadata.created) <= Retention::default().days * 86400
            && d.metadata.index_tree == index_tree
            && d.metadata.work_tree == work_tree
            && d.metadata.head == before.head
            && d.metadata.branch == before.branch
            && d.metadata.files == files
    }) {
        cleanup_in(
            &ctx,
            &existing,
            git::now(),
            Retention::default(),
            keep,
            &active,
        )?;
        return Ok(Some(last.clone()));
    }
    let metadata = Metadata {
        version: 1,
        created: git::now(),
        reason: reason
            .chars()
            .filter(|c| !c.is_control())
            .take(160)
            .collect(),
        worktree: ctx.id.clone(),
        head: before.head,
        branch: before.branch,
        index_tree,
        work_tree,
        files,
    };
    let bytes = serde_json::to_vec(&metadata).map_err(|e| e.to_string())?;
    if bytes.len() > 1024 * 1024 {
        return Err(crate::i18n::text(
            "backend-draft-metadata-exceeds-1-mib-make-a-manual-backup-91327e",
        )
        .into());
    }
    let meta = ctx.blob(bytes, &active)?;
    let input = format!(
        "100644 blob {meta}\tmetadata.json\0\
        040000 tree {}\tindex\0\
        040000 tree {}\twork\0",
        metadata.index_tree, metadata.work_tree
    )
    .into_bytes();
    let root = ctx.text(&["mktree", "-z"], input, &active)?;
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let reference = format!(
        "{PREFIX}{}/{nanos:039}-{:010}-{:010}",
        ctx.id,
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    );
    let id = Id {
        reference,
        tree: root,
    };
    ctx.source(
        &["update-ref", "--no-deref", "--stdin"],
        format!("create {} {}\n", id.reference, id.tree).into_bytes(),
        &[0],
        &active,
    )?;
    let draft = Draft { id, metadata };
    cleanup_in(
        &ctx,
        &existing,
        git::now(),
        Retention {
            count: Retention::default().count.saturating_sub(1),
            ..Default::default()
        },
        keep,
        &active,
    )?;
    Ok(Some(draft))
}

pub(super) fn source(ctx: &Context, active: &dyn Fn() -> bool) -> Result<Source, String> {
    let raw = ctx.source(&["ls-files", "-v", "--stage", "-z"], vec![], &[0], active)?;
    let (mut index, mut skip) = (BTreeMap::new(), BTreeSet::new());
    for record in raw.split(|b| *b == 0).filter(|r| !r.is_empty()) {
        let record = std::str::from_utf8(record)
            .map_err(|_| crate::i18n::text("backend-draft-paths-must-be-utf-8-7ae280"))?;
        let (info, path) = record
            .split_once('\t')
            .ok_or(crate::i18n::text("backend-cannot-read-index-entry-6979bd"))?;
        valid_path(path)?;
        let parts: Vec<_> = info.split_whitespace().collect();
        let [tag, mode, oid, stage] = parts.as_slice() else {
            return Err(crate::i18n::text("backend-cannot-read-index-entry-6979bd").into());
        };
        if *stage != "0" {
            return Err(crate::i18n::text(
                "backend-drafts-are-unavailable-while-the-index-contains-conflicts-resolve-6a5dd9",
            )
            .into());
        }
        if !matches!(*mode, "100644" | "100755" | "120000" | "160000") || !oid_valid(oid) {
            return Err(
                crate::i18n::text("backend-unsupported-index-entry-for-drafts-ed55d1").into(),
            );
        }
        if tag.eq_ignore_ascii_case("S") {
            skip.insert(path.to_owned());
        }
        index.insert(
            path.to_owned(),
            Entry {
                mode: (*mode).into(),
                oid: (*oid).into(),
            },
        );
        if index.len() > MAX_INDEX {
            return Err(crate::i18n::text(
                "backend-drafts-support-at-most-20000-index-entries-bd3f0a",
            )
            .into());
        }
    }
    let text = |args: &[&str]| -> Result<Option<String>, String> {
        let bytes = ctx.source(args, vec![], &[0, 1], active)?;
        let s = String::from_utf8(bytes)
            .map_err(|_| crate::i18n::text("backend-invalid-head-e1661b"))?;
        Ok((!s.trim().is_empty()).then(|| s.trim().to_owned()))
    };
    let cache = internal_file(ctx, &ctx.index_file, active)?.unwrap_or_default();
    let cache_snapshot =
        if !remote::is_remote(&ctx.repo) && cfg!(any(target_os = "macos", target_os = "linux")) {
            let snapshot = index_transaction::IndexSnapshot::capture(Path::new(&ctx.index_file))?;
            if snapshot.as_ref().map(|s| s.bytes()).unwrap_or_default() != cache {
                return Err(crate::i18n::text(
                    "backend-index-policy-changed-while-reading-source-3a58e5",
                )
                .into());
            }
            snapshot.map(std::sync::Arc::new)
        } else {
            None
        };
    Ok(Source {
        cache,
        cache_snapshot,
        shared: if ctx.shared_file.is_empty() {
            None
        } else {
            let name = ctx
                .shared_file
                .rsplit(['/', '\\'])
                .next()
                .ok_or(crate::i18n::text(
                    "backend-invalid-shared-index-path-cecec6",
                ))?;
            if !name.strip_prefix("sharedindex.").is_some_and(oid_valid) {
                return Err(crate::i18n::text("backend-invalid-shared-index-path-cecec6").into());
            }
            Some((
                name.into(),
                internal_file(ctx, &ctx.shared_file, active)?
                    .ok_or(crate::i18n::text("backend-shared-index-disappeared-8b3784"))?,
            ))
        },
        raw,
        index,
        skip,
        head: text(&["rev-parse", "--verify", "-q", "HEAD"])?,
        branch: text(&["symbolic-ref", "-q", "HEAD"])?,
    })
}
