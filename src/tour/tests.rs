use super::*;
#[test]
fn workflow_observations_do_not_verify_signatures_or_assert_a_policy() -> Result<(), String> {
    let object = "tree aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\nparent bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\nparent cccccccccccccccccccccccccccccccccccccccc\ngpgsig fixture-only-never-verified\n more fixture bytes\n\nfeat(ui): a contribution\n\nSigned-off-by: A <a@test.invalid>\n";
    let bytes = format!("{} commit {}\n{object}\n", "d".repeat(40), object.len());
    let mut w = Workflow::default();
    inspect_objects(bytes.as_bytes(), 1, &mut w)?;
    assert_eq!(
        (
            w.inspected,
            w.merges,
            w.conventional,
            w.signature_headers,
            w.signoffs
        ),
        (1, 1, 1, 1, 1)
    );
    assert!(inspect_objects(b"bad", 1, &mut w).is_err());
    Ok(())
}
#[test]
fn real_repository_tour_reads_documents_needs_and_one_cached_history_without_hooks()
-> Result<(), String> {
    let repo = crate::practice::create(crate::practice::Scenario::Published)?;
    std::fs::create_dir_all(repo.join(".github")).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(repo.join(".husky")).map_err(|e| e.to_string())?;
    for (path, text) in [
        ("README.md", "Read this project first."),
        ("CONTRIBUTING.md", "Review changes and sign off."),
        (".github/pull_request_template.md", "Describe validation."),
        (".gitattributes", "*.bin filter=lfs\n"),
        (".editorconfig", "root=true\n"),
        (
            ".gitmodules",
            "[submodule \"example\"]\n path=example\n url=https://example.invalid/project.git\n",
        ),
        (".husky/pre-commit", "echo NEVER-RUN\n"),
    ] {
        std::fs::write(repo.join(path), text).map_err(|e| e.to_string())?;
    }
    git::practice_run(&repo, &["add", "."])?;
    git::practice_run(
        &repo,
        &[
            "commit",
            "-m",
            "docs: first day guide\n\nSigned-off-by: Gitcraft Practice <practice@example.invalid>",
        ],
    )?;
    let h = Arc::new(git::activity::read(
        &repo,
        100,
        &|| true,
        &AtomicUsize::new(0),
    )?);
    let now = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_secs()
        / 86400) as i64;
    let result = read(&repo, Some(h.clone()), &|| true, &AtomicUsize::new(0), now)?;
    assert!(Arc::ptr_eq(&h, &result.history));
    assert!(result.needs.contains("Git LFS"));
    assert!(result.needs.contains("EditorConfig"));
    assert!(result.needs.contains("Submodules"));
    assert!(
        result
            .documents
            .iter()
            .any(|d| d.path == "README.md" && d.text.contains("Read this"))
    );
    assert!(result.workflow.signoffs > 0);
    assert_eq!(result.workflow.inspected, h.commits.len());
    assert!(!result.folders.is_empty());
    assert!(read(&repo, None, &|| false, &AtomicUsize::new(0), now).is_err());
    let _ = std::fs::remove_dir_all(repo.parent().ok_or("root")?);
    Ok(())
}

#[cfg(unix)]
#[test]
fn encoded_remote_tour_and_portable_snapshot_use_only_the_recording_local_launcher()
-> Result<(), String> {
    const CHILD: &str = "GITCRAFT_TOUR_RECORDING_REMOTE";
    if let Some(path) = std::env::var_os(CHILD) {
        let path = std::path::PathBuf::from(path);
        let encoded = crate::remote::encode(
            &crate::remote::Host::Ssh("tour-fixture.invalid".into()),
            path.to_str().ok_or("UTF-8 path")?,
        );
        let report = read(&encoded, None, &|| true, &AtomicUsize::new(0), 20000)?;
        assert!(
            report
                .documents
                .iter()
                .any(|d| d.path == "README.md" && d.text.contains("remote fixture"))
        );
        let setup = crate::portability::snapshot(&encoded, vec![1], Vec::new(), &|| true)?;
        assert_eq!(
            setup.author_email.as_deref(),
            Some("practice@example.invalid")
        );
        assert_eq!(setup.previous_path, encoded.to_string_lossy());
        let rejected = crate::portability::restore(&setup, &encoded, Some(&path), None, &|| true);
        assert!(!rejected.complete);
        assert!(rejected.messages.join(" ").contains("local absolute"));
        return Ok(());
    }
    let repo = crate::practice::create(crate::practice::Scenario::Published)?;
    let root = repo.parent().ok_or("root")?.to_owned();
    std::fs::write(repo.join("README.md"), "Only the remote fixture's README.")
        .map_err(|e| e.to_string())?;
    git::practice_run(&repo, &["add", "README.md"])?;
    git::practice_run(&repo, &["commit", "-m", "docs: remote fixture"])?;
    let bin = root.join("recording-bin");
    std::fs::create_dir(&bin).map_err(|e| e.to_string())?;
    let ssh = bin.join("ssh");
    let marker = root.join("recording-launcher-used");
    std::fs::write(
        &ssh,
        format!(
            "#!/bin/sh\nprintf '%s\\n' recording >> {}\nexec /bin/sh\n",
            crate::remote::sh_quote(&marker.to_string_lossy())
        ),
    )
    .map_err(|e| e.to_string())?;
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&ssh, std::fs::Permissions::from_mode(0o700))
        .map_err(|e| e.to_string())?;
    let mut paths = vec![bin];
    paths.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    let output=std::process::Command::new(std::env::current_exe().map_err(|e|e.to_string())?).args(["--exact","tour::tests::encoded_remote_tour_and_portable_snapshot_use_only_the_recording_local_launcher","--nocapture"]).env(CHILD,&repo).env("PATH",std::env::join_paths(paths).map_err(|e|e.to_string())?).output().map_err(|e|e.to_string())?;
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(marker.exists());
    std::fs::remove_dir_all(root).map_err(|e| e.to_string())?;
    Ok(())
}
