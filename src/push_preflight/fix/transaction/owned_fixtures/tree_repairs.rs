fn full_fix_case(root: &Path) -> Result<(), String> {
    let f = Fixture::new(root, "files", "sha1")?;
    let receiver = root.join("owned-receiver.git");
    run(
        &f.repo,
        &["init", "--template=", "--bare", &receiver.to_string_lossy()],
        b"",
        None,
    )?;
    run(
        &f.repo,
        &["remote", "add", "origin", &receiver.to_string_lossy()],
        b"",
        None,
    )?;
    run(
        &f.repo,
        &["push", "--set-upstream", "origin", "main"],
        b"",
        None,
    )?;
    write(
        f.repo.join("private.txt"),
        b"password=fixture_dummy_value_123\n",
    )?;
    run(&f.repo, &["add", "--", "private.txt"], b"", None)?;
    run(
        &f.repo,
        &["commit", "--no-gpg-sign", "-m", "Owned outgoing fixture"],
        b"",
        None,
    )?;
    let outgoing = run(&f.repo, &["rev-parse", "HEAD"], b"", None)?;
    f.observer("full-fix")?;
    let before = snapshot(&f.repo)?;
    let profile = crate::settings::profiles::UserProfile {
        author_name: "Gate2 Fixture".into(),
        author_email: "gate2@example.invalid".into(),
        ..Default::default()
    };
    let plan = crate::push_preflight::scan(&f.repo, vec![vec!["push".into()]], profile, &|| true)?;
    assert!(
        plan.findings
            .iter()
            .any(|finding| finding.kind == crate::push_preflight::Kind::Secret)
    );
    let result = crate::push_preflight::fix(&f.repo, &plan, "secrets");
    let after = snapshot(&f.repo)?;
    write(root.join("case.json"), serde_json::to_vec_pretty(&json!({"case":"full-fix","before":before,"after":after,"result":result,"secret_file_exists_after":f.repo.join("private.txt").exists(),"receiver_head":run(&receiver,&["rev-parse","refs/heads/main"],b"",None)?,"baseline":f.old})).map_err(|error| error.to_string())?)?;
    result?;
    let new = run(&f.repo, &["rev-parse", "HEAD"], b"", None)?;
    assert_ne!(new, outgoing);
    assert_eq!(run(&f.repo, &["rev-parse", "HEAD^"], b"", None)?, f.old);
    assert!(!f.repo.join("private.txt").exists());
    assert!(
        run(
            &f.repo,
            &["status", "--porcelain=v1", "--untracked-files=all"],
            b"",
            None
        )?
        .is_empty()
    );
    assert_eq!(
        run(&receiver, &["rev-parse", "refs/heads/main"], b"", None)?,
        f.old
    );
    for phase in ["prepared", "checkout", "committed"] {
        assert_eq!(
            fs::read(root.join(format!("observed-{phase}.ok")))
                .map_err(|error| error.to_string())?,
            format!("refs/heads/main\n{outgoing}\n{new}\n").as_bytes()
        );
    }
    assert_eq!(
        fs::read(root.join("original-input-prepared")).map_err(|error| error.to_string())?,
        format!("{outgoing} {new} refs/heads/main\n").as_bytes()
    );
    assert_eq!(
        fs::read(root.join("original-args-prepared")).map_err(|error| error.to_string())?,
        b"prepared\0"
    );
    assert!(
        crate::push_preflight::scan(&f.repo, vec![vec!["push".into()]], plan.profile, &|| true)?
            .findings
            .iter()
            .all(|finding| finding.kind != crate::push_preflight::Kind::Secret)
    );
    Ok(())
}

fn split_case(root: &Path) -> Result<(), String> {
    let f = Fixture::new(root, "files", "sha1")?;
    run(&f.repo, &["update-index", "--split-index"], b"", None)?;
    let current = root.join("saved-pair");
    fs::create_dir(&current).map_err(|e| e.to_string())?;
    let index = f.repo.join(".git/index");
    let before = snapshot(&f.repo)?;
    let mut selected = String::new();
    let dependency = snapshot_pair(
        index.to_str().ok_or("Owned fixture path is not UTF-8")?,
        &current,
        |saved| {
            // Force an actual new dependency only after immutable before.index was captured.
            run(&f.repo, &["update-index", "--no-split-index"], b"", None)?;
            run(
                &f.repo,
                &[
                    "update-index",
                    "--add",
                    "--cacheinfo",
                    &format!(
                        "100755,{},file.txt",
                        run(
                            &f.repo,
                            &["rev-parse", &format!("{}:file.txt", f.old)],
                            b"",
                            None
                        )?
                    ),
                ],
                b"",
                None,
            )?;
            run(&f.repo, &["update-index", "--split-index"], b"", None)?;
            selected = path_line(local(
                &f.repo,
                &["rev-parse", "--path-format=absolute", "--shared-index-path"],
                b"",
                &[("GIT_INDEX_FILE", saved.to_string_lossy().into_owned())],
            )?)?;
            Ok(Some(selected.clone()))
        },
    )?;
    let live = run(
        &f.repo,
        &["rev-parse", "--path-format=absolute", "--shared-index-path"],
        b"",
        None,
    )?;
    assert_ne!(selected, live, "fixture did not rotate dependency");
    assert_eq!(dependency.as_deref(), Some(selected.as_str()));
    assert_eq!(
        fs::read(current.join("before.shared-index")).map_err(|error| error.to_string())?,
        fs::read(&selected).map_err(|error| error.to_string())?
    );
    assert_eq!(before["shared_path"], selected);
    // Only remove this fixture's now-unreferenced dependency; live index uses another.
    fs::remove_file(&selected).map_err(|e| e.to_string())?;
    let missing = root.join("missing-pair");
    fs::create_dir(&missing).map_err(|e| e.to_string())?;
    let result = snapshot_pair(
        current
            .join("before.index")
            .to_str()
            .ok_or("Owned fixture path is not UTF-8")?,
        &missing,
        |saved| {
            let bytes = local(
                &f.repo,
                &["rev-parse", "--path-format=absolute", "--shared-index-path"],
                b"",
                &[("GIT_INDEX_FILE", saved.to_string_lossy().into_owned())],
            )?;
            path_line(bytes).map(Some)
        },
    );
    let after = snapshot(&f.repo)?;
    write(root.join("case.json"),serde_json::to_vec_pretty(&json!({"case":"split-index","before":before,"after":after,"saved_dependency":selected,"live_dependency":live,"missing_result":result,"saved_pair":evidence(&current)})).map_err(|error| error.to_string())?)?;
    assert!(result.is_err(), "missing dependency unexpectedly qualified");
    assert_eq!(after["head"], before["head"]);
    assert_eq!(after["work"], before["work"]);
    Ok(())
}

// Diagnostic only: native Git HEAD update in an owned fixture, without invoking
// changed production code or checking out any working tree.
