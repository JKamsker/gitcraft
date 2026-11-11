//! Retention.

use super::*;

#[test]
pub(super) fn tree_roots_survive_gc_restart_and_do_not_change_history_branches_reflog_or_normal_push()
-> Result<(), String> {
    let f = Fixture::new()?;
    f.write("file", b"base\n")?;
    f.commit()?;
    git::run(Some(&f.0), &["branch", "side"])?;
    let log = git::log(&f.0, None, Default::default(), 0, 100)?
        .into_iter()
        .map(|c| c.hash)
        .collect::<Vec<_>>();
    let native = git::run(Some(&f.0), &["log", "--all", "--format=%H"])?;
    let count = git::run(Some(&f.0), &["rev-list", "--all", "--count"])?;
    let branches = git::run(
        Some(&f.0),
        &["branch", "--all", "--format=%(refname) %(objectname)"],
    )?;
    let merged = git::run(
        Some(&f.0),
        &["branch", "--merged", "HEAD", "--format=%(refname)"],
    )?;
    let reflog = git::run(Some(&f.0), &["reflog", "--all", "--format=%H %gs"])?;
    let head = git::head(&f.0);
    f.write("file", b"uncommitted old content\n")?;
    let draft = capture(&f.0, "Pause", &|| true)?.ok_or("no draft")?;
    let old_blob = draft
        .metadata
        .files
        .iter()
        .find(|p| p.path == "file")
        .and_then(|p| p.work.as_ref())
        .ok_or("no file")?
        .oid
        .clone();
    assert_eq!(
        git::run(Some(&f.0), &["cat-file", "-t", &draft.id.tree])?.trim(),
        "tree"
    );
    git::run(Some(&f.0), &["gc", "--prune=now"])?;
    assert_eq!(list(&f.0, &|| true)?[0].id, draft.id);
    assert!(git::run(Some(&f.0), &["cat-file", "-e", &old_blob]).is_ok());
    assert_eq!(
        git::log(&f.0, None, Default::default(), 0, 100)?
            .into_iter()
            .map(|c| c.hash)
            .collect::<Vec<_>>(),
        log
    );
    assert_eq!(
        git::run(Some(&f.0), &["log", "--all", "--format=%H"])?,
        native
    );
    assert_eq!(
        git::run(Some(&f.0), &["rev-list", "--all", "--count"])?,
        count
    );
    assert_eq!(
        git::run(
            Some(&f.0),
            &["branch", "--all", "--format=%(refname) %(objectname)"]
        )?,
        branches
    );
    assert_eq!(
        git::run(
            Some(&f.0),
            &["branch", "--merged", "HEAD", "--format=%(refname)"]
        )?,
        merged
    );
    assert_eq!(
        git::run(Some(&f.0), &["reflog", "--all", "--format=%H %gs"])?,
        reflog
    );
    assert_eq!(git::head(&f.0), head);
    assert!(git::stashes(&f.0)?.is_empty());
    assert_eq!(git::refs(&f.0)?.local.len(), 2);
    let bare = f.0.join(".git/test-push.git");
    git::run(
        None,
        &[
            "init",
            "--bare",
            "--template=",
            bare.to_str().ok_or("path")?,
        ],
    )?;
    git::run(Some(&f.0), &["push", "--all", bare.to_str().ok_or("path")?])?;
    assert!(
        git::run(
            Some(&bare),
            &["for-each-ref", "--format=%(refname)", "refs/gitcraft"]
        )?
        .is_empty()
    );
    assert_eq!(git::refs(&bare)?.local.len(), 2);
    Ok(())
}

#[test]
pub(super) fn retention_expires_owned_refs_with_cas_and_preserves_unknown_versions()
-> Result<(), String> {
    let f = Fixture::new()?;
    f.write("file", b"base\n")?;
    f.commit()?;
    let mut saved = Vec::new();
    for n in 0..3 {
        f.write("file", format!("version {n}\n").as_bytes())?;
        saved.push(capture(&f.0, "Pause", &|| true)?.ok_or("no draft")?);
    }
    let ctx = Context::new(&f.0, &|| true)?;
    let listed = list_in(&ctx, &|| true)?;
    assert_eq!(listed[0].id, saved[2].id);
    cleanup_in(
        &ctx,
        &listed,
        git::now(),
        Retention { count: 2, days: 14 },
        None,
        &|| true,
    )?;
    assert_eq!(list_in(&ctx, &|| true)?.len(), 2);
    assert!(
        git::run(
            Some(&f.0),
            &["rev-parse", "--verify", &saved[0].id.reference]
        )
        .is_err()
    );
    // A ref replaced after listing must never be deleted with the stale old object ID.
    git::run(
        Some(&f.0),
        &["update-ref", &saved[1].id.reference, &saved[2].id.tree],
    )?;
    assert!(
        cleanup_in(
            &ctx,
            &[saved[1].clone()],
            git::now(),
            Retention { count: 0, days: 14 },
            None,
            &|| true
        )
        .is_err()
    );
    assert_eq!(
        git::run(Some(&f.0), &["rev-parse", &saved[1].id.reference])?.trim(),
        saved[2].id.tree
    );
    let mut future = saved[2].metadata.clone();
    future.version = 2;
    let blob = ctx.blob(
        serde_json::to_vec(&future).map_err(|e| e.to_string())?,
        &|| true,
    )?;
    let input = format!(
        "100644 blob {blob}\tmetadata.json\0\
040000 tree {}\tindex\0\
040000 tree {}\twork\0",
        future.index_tree, future.work_tree
    )
    .into_bytes();
    let root = ctx.text(&["mktree", "-z"], input, &|| true)?;
    let future_ref = format!("{PREFIX}{}/999-999-999", ctx.id);
    ctx.source(&["update-ref", &future_ref, &root], vec![], &[0], &|| true)?;
    let listed = list_in(&ctx, &|| true)?;
    cleanup_in(
        &ctx,
        &listed,
        git::now() + 15 * 86400,
        Retention::default(),
        None,
        &|| true,
    )?;
    assert!(list_in(&ctx, &|| true)?.is_empty());
    assert_eq!(
        git::run(Some(&f.0), &["rev-parse", &future_ref])?.trim(),
        root
    );
    Ok(())
}
