use super::*;
use std::path::PathBuf;
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Result<Self, String> {
        static ID: AtomicU64 = AtomicU64::new(0);
        let p = std::env::temp_dir().join(format!(
            "gitcraft-draft-test-{}-{}",
            std::process::id(),
            ID.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&p).map_err(|e| e.to_string())?;
        for args in [
            vec!["init", "-q"],
            vec!["config", "user.name", "Test"],
            vec!["config", "user.email", "test@example.com"],
            vec!["config", "commit.gpgSign", "false"],
        ] {
            git::run(Some(&p), &args)?;
        }
        Ok(Self(p))
    }
    fn write(&self, path: &str, data: &[u8]) -> Result<(), String> {
        let p = self.0.join(path);
        std::fs::create_dir_all(p.parent().ok_or("no parent")?)
            .and_then(|_| std::fs::write(p, data))
            .map_err(|e| e.to_string())
    }
    fn commit(&self) -> Result<(), String> {
        git::run(Some(&self.0), &["add", "-A"])?;
        git::run(Some(&self.0), &["commit", "-qm", "base"]).map(|_| ())
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
#[test]
fn mixed_index_work_and_untracked_are_independent_and_ignored_files_stay_out() -> Result<(), String>
{
    let f = Fixture::new()?;
    f.write("file", b"base\n")?;
    f.write(".gitignore", b"*.secret\n")?;
    f.commit()?;
    f.write("file", b"staged\n")?;
    git::run(Some(&f.0), &["add", "file"])?;
    f.write("file", b"unstaged\n")?;
    f.write("new file", b"untracked\0binary")?;
    f.write("private.secret", b"never copy")?;
    let index = std::fs::read(f.0.join(".git/index")).map_err(|e| e.to_string())?;
    let snapshot = capture(&f.0, "Pause", &|| true)?.ok_or("no snapshot")?;
    assert!(
        snapshot
            .metadata
            .files
            .iter()
            .any(|p| p.path == "new file" && p.untracked)
    );
    assert!(
        !snapshot
            .metadata
            .files
            .iter()
            .any(|p| p.path == "private.secret")
    );
    let got = git::run_bytes(
        Some(&f.0),
        &[
            "cat-file",
            "blob",
            &format!("{}:file", snapshot.metadata.index_tree),
        ],
    )?;
    assert_eq!(got, b"staged\n");
    assert_eq!(
        git::run_bytes(
            Some(&f.0),
            &[
                "cat-file",
                "blob",
                &format!("{}:file", snapshot.metadata.work_tree)
            ]
        )?,
        b"unstaged\n"
    );
    assert_eq!(
        std::fs::read(f.0.join(".git/index")).map_err(|e| e.to_string())?,
        index
    );
    assert_eq!(list(&f.0, &|| true)?[0].id, snapshot.id);
    assert_eq!(
        capture(&f.0, "Pause again", &|| true)?
            .ok_or("no snapshot")?
            .id,
        snapshot.id
    );
    Ok(())
}

#[test]
fn selected_hunks_recover_without_changing_staged_state_and_have_a_safety_version()
-> Result<(), String> {
    let f = Fixture::new()?;
    let base = (0..28).map(|i| format!("line {i}\n")).collect::<String>();
    f.write("file", base.as_bytes())?;
    f.commit()?;
    let saved = base
        .replace("line 1\n", "saved first\n")
        .replace("line 24\n", "saved second\n");
    f.write("file", saved.as_bytes())?;
    let draft = capture(&f.0, "Pause", &|| true)?.ok_or("no draft")?;
    let newer = base
        .replace("line 1\n", "newer first\n")
        .replace("line 24\n", "newer second\n");
    f.write("file", newer.as_bytes())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(f.0.join("file"), std::fs::Permissions::from_mode(0o600))
            .map_err(|e| e.to_string())?;
    }
    let index = std::fs::read(f.0.join(".git/index")).map_err(|e| e.to_string())?;
    let view = preview(&f.0, &draft.id, "file", false, &|| true)?;
    assert_eq!(view.hunks.len(), 2);
    let backup = restore(&f.0, &view, &[0], &|| true)?;
    assert_eq!(
        std::fs::read_to_string(f.0.join("file")).map_err(|e| e.to_string())?,
        newer.replace("newer first\n", "saved first\n")
    );
    assert_eq!(
        std::fs::read(f.0.join(".git/index")).map_err(|e| e.to_string())?,
        index
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(f.0.join("file"))
                .map_err(|e| e.to_string())?
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
    let undo = preview(&f.0, &backup, "file", false, &|| true)?;
    let selected = (0..undo.hunks.len()).collect::<Vec<_>>();
    restore(&f.0, &undo, &selected, &|| true)?;
    assert_eq!(
        std::fs::read(f.0.join("file")).map_err(|e| e.to_string())?,
        newer.as_bytes()
    );
    assert_eq!(
        std::fs::read(f.0.join(".git/index")).map_err(|e| e.to_string())?,
        index
    );
    Ok(())
}

#[test]
fn binary_empty_files_deletions_and_index_layer_recovery_preserve_the_real_index()
-> Result<(), String> {
    let f = Fixture::new()?;
    f.write("file", b"base\n")?;
    f.commit()?;
    f.write("file", b"staged\n")?;
    git::run(Some(&f.0), &["add", "file"])?;
    f.write("file", b"\xff\xfe\x80binary without nul")?;
    f.write("empty", b"")?;
    let draft = capture(&f.0, "Binary", &|| true)?.ok_or("no draft")?;
    f.write("file", b"different\0binary")?;
    std::fs::remove_file(f.0.join("empty")).map_err(|e| e.to_string())?;
    let index = std::fs::read(f.0.join(".git/index")).map_err(|e| e.to_string())?;
    let view = preview(&f.0, &draft.id, "file", false, &|| true)?;
    assert!(view.binary);
    restore(&f.0, &view, &[0], &|| true)?;
    assert_eq!(
        std::fs::read(f.0.join("file")).map_err(|e| e.to_string())?,
        b"\xff\xfe\x80binary without nul"
    );
    let view = preview(&f.0, &draft.id, "file", true, &|| true)?;
    restore(&f.0, &view, &[0], &|| true)?;
    assert_eq!(
        std::fs::read(f.0.join("file")).map_err(|e| e.to_string())?,
        b"staged\n"
    );
    let view = preview(&f.0, &draft.id, "empty", false, &|| true)?;
    restore(&f.0, &view, &[0], &|| true)?;
    assert_eq!(
        std::fs::metadata(f.0.join("empty"))
            .map_err(|e| e.to_string())?
            .len(),
        0
    );
    std::fs::remove_file(f.0.join("file")).map_err(|e| e.to_string())?;
    let deleted = capture(&f.0, "Deleted", &|| true)?.ok_or("no deletion draft")?;
    f.write("file", b"keep until confirmed")?;
    let view = preview(&f.0, &deleted.id, "file", false, &|| true)?;
    restore(&f.0, &view, &[0], &|| true)?;
    assert!(!f.0.join("file").exists());
    assert_eq!(
        std::fs::read(f.0.join(".git/index")).map_err(|e| e.to_string())?,
        index
    );
    Ok(())
}

#[test]
fn recovery_refuses_stale_files_indexes_refs_and_new_ignore_rules() -> Result<(), String> {
    let f = Fixture::new()?;
    f.write("file", b"base\n")?;
    f.commit()?;
    f.write("file", b"saved\n")?;
    f.write("new", b"old untracked")?;
    let draft = capture(&f.0, "Pause", &|| true)?.ok_or("no draft")?;
    f.write("file", b"current\n")?;
    let view = preview(&f.0, &draft.id, "file", false, &|| true)?;
    f.write("file", b"edited after preview\n")?;
    assert!(restore(&f.0, &view, &[0], &|| true).is_err());
    assert_eq!(
        std::fs::read(f.0.join("file")).map_err(|e| e.to_string())?,
        b"edited after preview\n"
    );
    let view = preview(&f.0, &draft.id, "file", false, &|| true)?;
    git::run(Some(&f.0), &["add", "file"])?;
    assert!(restore(&f.0, &view, &[0], &|| true).is_err());
    let view = preview(&f.0, &draft.id, "file", false, &|| true)?;
    git::run(Some(&f.0), &["update-ref", "-d", &draft.id.reference])?;
    assert!(restore(&f.0, &view, &[0], &|| true).is_err());
    git::run(
        Some(&f.0),
        &["update-ref", &draft.id.reference, &draft.id.tree],
    )?;
    f.write(".gitignore", b"new\n")?;
    assert!(preview(&f.0, &draft.id, "new", false, &|| true).is_err());
    assert_eq!(
        std::fs::read(f.0.join("new")).map_err(|e| e.to_string())?,
        b"old untracked"
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn configured_filters_hooks_fsmonitor_and_signing_never_run_for_drafts_or_recovery()
-> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new()?;
    f.write("file", b"base\n")?;
    f.write(
        ".gitattributes",
        b"file filter=trap diff=trap merge=trap text eol=crlf\n",
    )?;
    f.commit()?;
    let marker = f.0.join(".git/executed");
    let script = f.0.join(".git/trap");
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\nprintf ran >> {}\nexit 1\n",
            remote::sh_quote(marker.to_str().ok_or("path")?)
        ),
    )
    .map_err(|e| e.to_string())?;
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700))
        .map_err(|e| e.to_string())?;
    for key in [
        "filter.trap.clean",
        "filter.trap.smudge",
        "filter.trap.process",
        "diff.trap.command",
        "diff.trap.textconv",
        "merge.trap.driver",
        "core.fsmonitor",
        "gpg.program",
        "core.sshCommand",
    ] {
        git::run(Some(&f.0), &["config", key, script.to_str().ok_or("path")?])?;
    }
    git::run(Some(&f.0), &["config", "filter.trap.required", "true"])?;
    git::run(Some(&f.0), &["config", "commit.gpgSign", "true"])?;
    std::fs::copy(&script, f.0.join(".git/hooks/reference-transaction"))
        .map_err(|e| e.to_string())?;
    f.write("file", b"saved\r\n")?;
    let index = std::fs::read(f.0.join(".git/index")).map_err(|e| e.to_string())?;
    let draft = capture(&f.0, "Pause", &|| true)?.ok_or("no draft")?;
    f.write("file", b"new\r\n")?;
    let view = preview(&f.0, &draft.id, "file", false, &|| true)?;
    restore(&f.0, &view, &[0], &|| true)?;
    assert_eq!(
        std::fs::read(f.0.join("file")).map_err(|e| e.to_string())?,
        b"saved\r\n"
    );
    assert!(!marker.exists());
    assert_eq!(
        std::fs::read(f.0.join(".git/index")).map_err(|e| e.to_string())?,
        index
    );
    // Verify the hostile source configuration is live, rather than a vacuous test setup.
    let _ = git::run(Some(&f.0), &["status", "--porcelain"]);
    assert!(marker.exists());
    Ok(())
}

#[test]
fn tree_roots_survive_gc_restart_and_do_not_change_history_branches_reflog_or_normal_push()
-> Result<(), String> {
    let f = Fixture::new()?;
    f.write("file", b"base\n")?;
    f.commit()?;
    git::run(Some(&f.0), &["branch", "side"])?;
    let log = git::log(&f.0, None, 0, 100)?
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
        git::log(&f.0, None, 0, 100)?
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
fn retention_expires_owned_refs_with_cas_and_preserves_unknown_versions() -> Result<(), String> {
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

#[cfg(unix)]
#[test]
fn changed_symlinks_and_conflicted_indexes_fail_without_following_targets_or_touching_index()
-> Result<(), String> {
    let f = Fixture::new()?;
    f.write("file", b"base\n")?;
    f.commit()?;
    let outside = f.0.join(".git/outside");
    std::fs::write(&outside, b"outside must stay intact").map_err(|e| e.to_string())?;
    std::fs::remove_file(f.0.join("file")).map_err(|e| e.to_string())?;
    std::os::unix::fs::symlink(&outside, f.0.join("file")).map_err(|e| e.to_string())?;
    let index = std::fs::read(f.0.join(".git/index")).map_err(|e| e.to_string())?;
    assert!(capture(&f.0, "Pause", &|| true).is_err());
    assert_eq!(
        std::fs::read(f.0.join(".git/index")).map_err(|e| e.to_string())?,
        index
    );
    assert_eq!(
        std::fs::read(&outside).map_err(|e| e.to_string())?,
        b"outside must stay intact"
    );
    assert!(list(&f.0, &|| true)?.is_empty());
    std::fs::remove_file(f.0.join("file")).map_err(|e| e.to_string())?;
    f.write("file", b"base\n")?;
    let oid = git::run(Some(&f.0), &["rev-parse", "HEAD:file"])?;
    let oid = oid.trim();
    let input = format!(
        "0 {}\tfile\n100644 {oid} 1\tfile\n100644 {oid} 2\tfile\n100644 {oid} 3\tfile\n",
        "0".repeat(40)
    );
    git::draft_bytes(
        &f.0,
        &["update-index", "--index-info"],
        &[],
        input.into_bytes(),
        &[0],
        &|| true,
    )?;
    let index = std::fs::read(f.0.join(".git/index")).map_err(|e| e.to_string())?;
    assert!(capture(&f.0, "Conflict", &|| true).is_err_and(|e| e.contains("conflicts")));
    assert_eq!(
        std::fs::read(f.0.join(".git/index")).map_err(|e| e.to_string())?,
        index
    );
    assert!(list(&f.0, &|| true)?.is_empty());
    Ok(())
}

#[test]
fn limits_invalid_paths_and_cancellation_leave_all_real_state_untouched() -> Result<(), String> {
    for path in [
        "../escape",
        "a/../b",
        ".git/config",
        "a/.Git/x",
        "C:/file",
        "a\\file",
        "line\nbreak",
        "a//b",
        "/absolute",
    ] {
        assert!(valid_path(path).is_err(), "{path}");
    }
    let f = Fixture::new()?;
    f.write("file", b"base\n")?;
    f.commit()?;
    let index = std::fs::read(f.0.join(".git/index")).map_err(|e| e.to_string())?;
    std::fs::File::create(f.0.join("large"))
        .and_then(|f| f.set_len(FILE_LIMIT as u64 + 1))
        .map_err(|e| e.to_string())?;
    assert!(capture(&f.0, "Large", &|| true).is_err());
    std::fs::remove_file(f.0.join("large")).map_err(|e| e.to_string())?;
    f.write("file", b"cancelled working data\n")?;
    let raw = git::draft_bytes(
        &f.0,
        &["hash-object", "--no-filters", "--stdin"],
        &[],
        b"cancelled working data\n".to_vec(),
        &[0],
        &|| true,
    )?;
    let hash = String::from_utf8(raw).map_err(|e| e.to_string())?;
    let hash = hash.trim();
    let object = f.0.join(".git/objects").join(&hash[..2]).join(&hash[2..]);
    assert!(!object.exists());
    assert!(capture(&f.0, "Cancelled", &|| !object.exists()).is_err());
    assert!(list(&f.0, &|| true)?.is_empty());
    assert_eq!(
        std::fs::read(f.0.join(".git/index")).map_err(|e| e.to_string())?,
        index
    );
    assert_eq!(
        std::fs::read(f.0.join("file")).map_err(|e| e.to_string())?,
        b"cancelled working data\n"
    );
    Ok(())
}

#[test]
fn private_index_cache_handles_split_and_sparse_indexes_without_rewriting_the_real_index()
-> Result<(), String> {
    let f = Fixture::new()?;
    f.write("keep/file", b"base\n")?;
    f.write("omit/file", b"outside sparse checkout\n")?;
    f.commit()?;
    git::run(Some(&f.0), &["update-index", "--split-index"])?;
    git::run(
        Some(&f.0),
        &["update-index", "--assume-unchanged", "keep/file"],
    )?;
    f.write("keep/file", b"hidden working edit\n")?;
    let index = std::fs::read(f.0.join(".git/index")).map_err(|e| e.to_string())?;
    let saved = capture(&f.0, "Split index", &|| true)?.ok_or("No split-index draft")?;
    assert_eq!(
        git::run_bytes(
            Some(&f.0),
            &[
                "cat-file",
                "blob",
                &format!("{}:keep/file", saved.metadata.work_tree)
            ]
        )?,
        b"hidden working edit\n"
    );
    assert_eq!(
        std::fs::read(f.0.join(".git/index")).map_err(|e| e.to_string())?,
        index
    );
    git::run(
        Some(&f.0),
        &["update-index", "--no-assume-unchanged", "keep/file"],
    )?;
    git::run(Some(&f.0), &["restore", "keep/file"])?;
    git::run(Some(&f.0), &["update-index", "--no-split-index"])?;
    git::run(
        Some(&f.0),
        &["sparse-checkout", "set", "--cone", "--sparse-index", "keep"],
    )?;
    f.write("keep/file", b"sparse edit\n")?;
    let index = std::fs::read(f.0.join(".git/index")).map_err(|e| e.to_string())?;
    let saved = capture(&f.0, "Sparse index", &|| true)?.ok_or("No sparse-index draft")?;
    assert_eq!(
        git::run_bytes(
            Some(&f.0),
            &[
                "cat-file",
                "blob",
                &format!("{}:keep/file", saved.metadata.work_tree)
            ]
        )?,
        b"sparse edit\n"
    );
    assert!(
        !saved
            .metadata
            .files
            .iter()
            .any(|file| file.path == "omit/file")
    );
    assert!(!f.0.join("omit/file").exists());
    assert_eq!(
        std::fs::read(f.0.join(".git/index")).map_err(|e| e.to_string())?,
        index
    );
    Ok(())
}

#[test]
fn recovery_detects_intent_to_add_changes_even_when_index_object_records_match()
-> Result<(), String> {
    let f = Fixture::new()?;
    f.write("file", b"base\n")?;
    f.commit()?;
    f.write("empty", b"")?;
    git::run(Some(&f.0), &["add", "-N", "empty"])?;
    f.write("file", b"saved\n")?;
    let saved = capture(&f.0, "Intent to add", &|| true)?.ok_or("No draft")?;
    f.write("file", b"newer\n")?;
    let view = preview(&f.0, &saved.id, "file", false, &|| true)?;
    let records = git::run(Some(&f.0), &["ls-files", "-v", "--stage", "-z"])?;
    git::run(Some(&f.0), &["add", "empty"])?;
    assert_eq!(
        git::run(Some(&f.0), &["ls-files", "-v", "--stage", "-z"])?,
        records
    );
    assert!(restore(&f.0, &view, &[0], &|| true).is_err());
    assert_eq!(
        std::fs::read(f.0.join("file")).map_err(|e| e.to_string())?,
        b"newer\n"
    );
    Ok(())
}

#[test]
fn unchanged_large_tracked_files_are_not_copied_as_working_changes() -> Result<(), String> {
    let f = Fixture::new()?;
    std::fs::File::create(f.0.join("large"))
        .and_then(|file| file.set_len(FILE_LIMIT as u64 + 1))
        .map_err(|e| e.to_string())?;
    f.write("file", b"base\n")?;
    f.commit()?;
    f.write("file", b"small working edit\n")?;
    let index = std::fs::read(f.0.join(".git/index")).map_err(|e| e.to_string())?;
    let saved = capture(&f.0, "Small change", &|| true)?.ok_or("No draft")?;
    assert_eq!(saved.metadata.files.len(), 1);
    assert_eq!(saved.metadata.files[0].path, "file");
    assert_eq!(
        std::fs::read(f.0.join(".git/index")).map_err(|e| e.to_string())?,
        index
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn typed_remote_reader_quotes_paths_and_never_confuses_links_with_missing_files()
-> Result<(), String> {
    use std::process::Command;
    let f = Fixture::new()?;
    let root = f.0.join("space ' quote $(touch trap) ü");
    std::fs::create_dir(&root).map_err(|e| e.to_string())?;
    std::fs::write(root.join("literal ' $x ü"), b"bytes\0raw").map_err(|e| e.to_string())?;
    let read = |rel: &str, limit| -> Result<std::process::Output, String> {
        Command::new("sh")
            .arg("-c")
            .arg(read_script(root.to_str().ok_or("utf8")?, rel, limit)?)
            .output()
            .map_err(|e| e.to_string())
    };
    let out = read("literal ' $x ü", 20)?;
    assert!(out.status.success(), "{:?}", out.stderr);
    assert_eq!(out.stdout, b"F100644\nbytes\0raw");
    assert_eq!(read("absent/folder/file", 20)?.stdout, b"M\n");
    assert!(!read("literal ' $x ü", 2)?.stdout.ends_with(b"bytes\0raw"));
    std::os::unix::fs::symlink("absent", root.join("link")).map_err(|e| e.to_string())?;
    assert!(!read("link/file", 20)?.status.success());
    assert!(!read("link", 20)?.status.success());
    assert!(missing_inside(&root, "link/file").is_err());
    assert!(missing_inside(&root, "absent/file").is_ok());
    assert!(!f.0.join("trap").exists());
    Ok(())
}

#[test]
fn nested_untracked_repositories_are_excluded_without_hiding_outer_changes() -> Result<(), String> {
    let f = Fixture::new()?;
    f.write("file", b"base\n")?;
    f.commit()?;
    f.write("nested/private", b"separate repository content")?;
    git::run(Some(&f.0.join("nested")), &["init", "-q"])?;
    f.write("outer", b"saved outer bytes")?;
    let saved = capture(&f.0, "Pause", &|| true)?.ok_or("No draft")?;
    assert!(saved.metadata.files.iter().any(|file| file.path == "outer"));
    assert!(
        !saved
            .metadata
            .files
            .iter()
            .any(|file| file.path.starts_with("nested"))
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn missing_paths_below_symlinks_fail_closed_and_do_not_publish_deletions() -> Result<(), String> {
    let f = Fixture::new()?;
    f.write("directory/file", b"tracked bytes")?;
    f.commit()?;
    std::fs::remove_dir_all(f.0.join("directory")).map_err(|e| e.to_string())?;
    std::os::unix::fs::symlink("absent", f.0.join("directory")).map_err(|e| e.to_string())?;
    let index = std::fs::read(f.0.join(".git/index")).map_err(|e| e.to_string())?;
    assert!(capture(&f.0, "Pause", &|| true).is_err());
    assert!(git::run(Some(&f.0), &["for-each-ref", PREFIX])?.is_empty());
    assert_eq!(
        std::fs::read(f.0.join(".git/index")).map_err(|e| e.to_string())?,
        index
    );
    Ok(())
}
