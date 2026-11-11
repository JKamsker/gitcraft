// Restore. Included to preserve libtest shard names.
#[test]
fn repository_snapshot_and_restore_round_trip_remotes_author_hidden_refs_and_linked_worktrees()
-> Result<(), String> {
    let fixture = Fixture::new()?;
    let work = fixture.root.join("feature source");
    crate::git::practice_run(
        &fixture.repo,
        &[
            "worktree",
            "add",
            "-b",
            "feature",
            &work.to_string_lossy(),
            "main",
        ],
    )?;
    crate::git::practice_run(
        &fixture.repo,
        &["worktree", "lock", &work.to_string_lossy()],
    )?;
    crate::git::practice_run(
        &fixture.repo,
        &[
            "remote",
            "set-url",
            "origin",
            "https://user:SECRET@fixture.invalid/project.git?token=SECRET",
        ],
    )?;
    crate::git::practice_run(
        &fixture.repo,
        &[
            "config",
            "--local",
            "remote.origin.pushurl",
            "ssh://git:SECRET@fixture.invalid:2222/project.git",
        ],
    )?;
    crate::git::practice_run(
        &fixture.repo,
        &["config", "--local", "pull.rebase", "merges"],
    )?;
    crate::git::practice_run(
        &fixture.repo,
        &["config", "--local", "core.autocrlf", "input"],
    )?;
    crate::git::practice_run(
        &fixture.repo,
        &[
            "config",
            "--local",
            "include.path",
            "/fixture/NEVER_EXPORT_INCLUDE",
        ],
    )?;
    let mut repository = fixture.definition()?;
    repository.name = "restored".into();
    assert_eq!(
        repository.config.get("pull.rebase").map(String::as_str),
        Some("merges")
    );
    assert!(!repository.config.contains_key("include.path"));
    assert_eq!(
        repository.remotes[0].fetch,
        "https://fixture.invalid/project.git"
    );
    assert_eq!(
        repository.remotes[0].push,
        ["ssh://git@fixture.invalid:2222/project.git"]
    );
    assert_eq!(repository.worktrees.len(), 1);
    assert!(repository.worktrees[0].was_locked);
    let mut setup = Setup::new(&Settings::default(), &Default::default());
    setup.repositories.push(repository);
    let encoded = setup.encode()?;
    assert!(!String::from_utf8_lossy(&encoded).contains("SECRET"));
    let repository = Setup::decode(&encoded)?.repositories.remove(0);
    let parent = fixture.parent()?;
    let outcome = restore(&repository, &parent, Some(&fixture.repo), None, &|| true);
    assert!(outcome.complete, "{:?}", outcome.messages);
    let imported = &outcome.receipt.as_ref().ok_or("No restore receipt")?.root;
    assert_eq!(
        String::from_utf8_lossy(&crate::git::setup_run(
            imported,
            &["config", "--local", "pull.rebase"],
            &|| true
        )?)
        .trim(),
        "merges"
    );
    let receipt = outcome.receipt.ok_or("No receipt")?;
    let run = |args: &[&str]| {
        crate::git::setup_run(&receipt.root, args, &|| true)
            .map(|b| String::from_utf8_lossy(&b).trim().to_owned())
    };
    assert_eq!(run(&["symbolic-ref", "--short", "HEAD"])?, "main");
    assert_eq!(
        run(&["config", "--local", "user.email"])?,
        "practice@example.invalid"
    );
    assert_eq!(
        run(&["remote", "get-url", "origin"])?,
        repository.remotes[0].fetch
    );
    let trees = run(&["worktree", "list", "--porcelain"])?;
    assert!(trees.contains("refs/heads/feature"));
    assert!(trees.contains("locked"));
    let again = restore(
        &repository,
        &parent,
        Some(&fixture.repo),
        Some(receipt),
        &|| true,
    );
    assert!(again.complete, "{:?}", again.messages);
    Ok(())
}

#[test]
fn restoration_refuses_overwrite_traversal_and_missing_branch_without_substitution()
-> Result<(), String> {
    let fixture = Fixture::new()?;
    let parent = fixture.parent()?;
    let mut r = fixture.definition()?;
    r.name = "missing".into();
    r.branch = Some("not-in-source".into());
    let result = restore(&r, &parent, Some(&fixture.repo), None, &|| true);
    assert!(!result.complete);
    assert!(result.messages.join(" ").contains("no substitute"));
    let receipt = result.receipt.ok_or("Partial clone not retained")?;
    let before =
        crate::git::setup_run(&receipt.root, &["symbolic-ref", "--short", "HEAD"], &|| {
            true
        })?;
    assert_eq!(String::from_utf8_lossy(&before).trim(), "main");
    let existing = parent.join("existing");
    std::fs::create_dir(&existing).map_err(|e| e.to_string())?;
    std::fs::write(existing.join("precious"), "keep").map_err(|e| e.to_string())?;
    r.name = "existing".into();
    let result = restore(&r, &parent, Some(&fixture.repo), None, &|| true);
    assert!(!result.complete);
    assert_eq!(
        std::fs::read_to_string(existing.join("precious")).map_err(|e| e.to_string())?,
        "keep"
    );
    for name in [
        "../escape",
        "/outside",
        "CON.txt",
        "lpt9",
        "bad/name",
        ".hidden",
    ] {
        r.name = name.into();
        assert!(
            !restore(&r, &parent, Some(&fixture.repo), None, &|| true).complete,
            "{name}"
        );
    }
    Ok(())
}

#[test]
fn partial_worktree_restore_can_retry_but_changed_config_cannot() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let parent = fixture.parent()?;
    let mut r = fixture.definition()?;
    r.name = "partial".into();
    r.worktrees.push(Worktree {
        name: "feature".into(),
        previous_path: "/old/machine/feature".into(),
        branch: Some("feature".into()),
        commit: r.commit.clone().ok_or("No commit")?,
        was_locked: false,
    });
    let failed = restore(&r, &parent, Some(&fixture.repo), None, &|| true);
    assert!(!failed.complete);
    assert!(
        failed
            .messages
            .join(" ")
            .contains("Branch feature is missing")
    );
    let receipt = failed.receipt.ok_or("No partial receipt")?;
    crate::git::setup_run(&receipt.root, &["branch", "feature", "HEAD"], &|| true)?;
    let done = restore(&r, &parent, Some(&fixture.repo), Some(receipt), &|| true);
    assert!(done.complete, "{:?}", done.messages);
    let receipt = done.receipt.ok_or("No final receipt")?;
    crate::git::setup_run(
        &receipt.root,
        &["config", "--local", "core.hooksPath", "changed-by-user"],
        &|| true,
    )?;
    let rejected = restore(&r, &parent, Some(&fixture.repo), Some(receipt), &|| true);
    assert!(!rejected.complete);
    assert!(rejected.messages.join(" ").contains("clone/config changed"));
    Ok(())
}

#[test]
fn cancelled_restore_never_creates_a_destination() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let parent = fixture.parent()?;
    let r = fixture.definition()?;
    assert!(!restore(&r, &parent, Some(&fixture.repo), None, &|| false).complete);
    assert!(!parent.join(&r.name).exists());
    Ok(())
}

#[cfg(unix)]
#[test]
fn practice_export_and_local_restore_isolate_hostile_git_environment_and_profile()
-> Result<(), String> {
    const FLAG: &str = "GITCRAFT_PORTABILITY_ISOLATION";
    if std::env::var_os(FLAG).is_some() {
        let fixture = Fixture::new()?;
        let parent = fixture.parent()?;
        let caller =
            std::path::PathBuf::from(std::env::var_os("GIT_WORK_TREE").ok_or("No caller")?);
        let caller_head = crate::git::practice_run(&caller, &["rev-parse", "HEAD"])?;
        let profile = crate::connections::Context {
            error: Some("Hostile profile must not apply to Practice/local source".into()),
            ..Default::default()
        };
        profile.run(|| -> Result<(), String> {
            let mut r = fixture.definition()?;
            r.name = "safe-local".into();
            let result = restore(&r, &parent, Some(&fixture.repo), None, &|| true);
            assert!(result.complete, "{:?}", result.messages);
            let report = crate::tour::read(
                &fixture.repo,
                None,
                &|| true,
                &std::sync::atomic::AtomicUsize::new(0),
                20000,
            )?;
            assert_eq!(report.identity.1, "practice@example.invalid");
            Ok(())
        })?;
        assert_eq!(
            crate::git::practice_run(&caller, &["rev-parse", "HEAD"])?,
            caller_head
        );
        assert_eq!(
            std::fs::read_to_string(caller.join("precious")).map_err(|e| e.to_string())?,
            "untouched"
        );
        let marker = std::path::PathBuf::from(
            std::env::var_os("GITCRAFT_PORTABILITY_EXEC_MARKER").ok_or("No marker")?,
        );
        assert!(!marker.exists());
        return Ok(());
    }
    let fixture = Fixture::new()?;
    std::fs::write(fixture.repo.join("precious"), "untouched").map_err(|e| e.to_string())?;
    let marker = fixture.root.join("must-not-execute");
    let script = fixture.root.join("record-only-ssh");
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\nprintf unsafe > '{}'\nexit 7\n",
            marker.display()
        ),
    )
    .map_err(|e| e.to_string())?;
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700))
        .map_err(|e| e.to_string())?;
    let hooks = fixture.root.join("recording-hooks");
    std::fs::create_dir(&hooks).map_err(|e| e.to_string())?;
    for name in ["post-checkout", "post-merge"] {
        let hook = hooks.join(name);
        std::fs::copy(&script, &hook).map_err(|e| e.to_string())?;
        std::fs::set_permissions(hook, std::fs::Permissions::from_mode(0o700))
            .map_err(|e| e.to_string())?;
    }
    let global = fixture.root.join("hostile.gitconfig");
    std::fs::write(&global,format!("[url \"ssh://fixture.invalid/redirect/\"]\n insteadOf = /\n[core]\n hooksPath = {}\n sshCommand = {}\n",hooks.display(),script.display())).map_err(|e|e.to_string())?;
    let result=std::process::Command::new(std::env::current_exe().map_err(|e|e.to_string())?).args(["--exact","portability::tests::practice_export_and_local_restore_isolate_hostile_git_environment_and_profile","--nocapture"]).env(FLAG,"1").env("GIT_DIR",fixture.repo.join(".git")).env("GIT_WORK_TREE",&fixture.repo).env("GIT_CONFIG_GLOBAL",&global).env("GIT_CONFIG_COUNT","1").env("GIT_CONFIG_KEY_0","core.sshCommand").env("GIT_CONFIG_VALUE_0",&script).env("GIT_SSH_COMMAND",&script).env("GITCRAFT_PORTABILITY_EXEC_MARKER",&marker).output().map_err(|e|e.to_string())?;
    assert!(
        result.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(!marker.exists());
    Ok(())
}

#[test]
fn locked_worktree_retry_completes_after_a_later_missing_branch_without_relocking()
-> Result<(), String> {
    let fixture = Fixture::new()?;
    let parent = fixture.parent()?;
    let mut r = fixture.definition()?;
    r.name = "locked-retry".into();
    crate::git::practice_run(&fixture.repo, &["branch", "feature", "HEAD"])?;
    let commit = r.commit.clone().ok_or("No fixture commit")?;
    r.worktrees = vec![
        Worktree {
            name: "feature".into(),
            previous_path: "/old/feature".into(),
            branch: Some("feature".into()),
            commit: commit.clone(),
            was_locked: true,
        },
        Worktree {
            name: "later".into(),
            previous_path: "/old/later".into(),
            branch: Some("later".into()),
            commit,
            was_locked: false,
        },
    ];
    let failed = restore(&r, &parent, Some(&fixture.repo), None, &|| true);
    assert!(!failed.complete);
    assert!(
        failed
            .messages
            .join(" ")
            .contains("Branch later is missing")
    );
    let receipt = failed.receipt.ok_or("Missing partial locked receipt")?;
    let locks = crate::git::setup_run(
        &receipt.root,
        &["worktree", "list", "--porcelain", "-z"],
        &|| true,
    )?;
    assert!(locks.split(|b| *b == 0).any(|field| field == b"locked"));
    crate::git::setup_run(&receipt.root, &["branch", "later", "HEAD"], &|| true)?;
    let done = restore(&r, &parent, Some(&fixture.repo), Some(receipt), &|| true);
    assert!(done.complete, "{:?}", done.messages);
    Ok(())
}

#[test]
fn worktree_lock_detection_uses_directory_identity_for_native_and_git_spellings()
-> Result<(), String> {
    let fixture = Fixture::new()?;
    let directory = fixture.root.join("locked");
    std::fs::create_dir(&directory).map_err(|e| e.to_string())?;
    #[cfg(not(windows))]
    let alias = directory.join("..").join("locked");
    let canonical = directory.canonicalize().map_err(|e| e.to_string())?;
    // On Windows this is the real Git-style C:/... form against Rust's canonical
    // \\?\C:\... path; other platforms exercise an equally nonidentical alias.
    #[cfg(windows)]
    let git_path = {
        let text = canonical.to_string_lossy().replace('\\', "/");
        if let Some(rest) = text.strip_prefix("//?/UNC/") {
            format!("//{rest}")
        } else {
            text.strip_prefix("//?/").unwrap_or(&text).to_owned()
        }
    };
    #[cfg(not(windows))]
    let git_path = alias.to_string_lossy().into_owned();
    let record = format!(
        "worktree {git_path}\0HEAD {}\0locked imported\0\0",
        "a".repeat(40)
    );
    assert!(restore::worktree_is_locked(record.as_bytes(), &canonical)?);
    assert!(!restore::worktree_is_locked(
        record.as_bytes(),
        &fixture.repo
    )?);
    Ok(())
}
