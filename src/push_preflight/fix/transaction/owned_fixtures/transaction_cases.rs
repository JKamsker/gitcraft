fn retain_acks(hook: &Hook, root: &Path) -> Result<(), String> {
    for phase in ["prepared", "checkout", "committed"] {
        let path = hook.directory.join(format!("{phase}.ok"));
        // Empty placeholders cannot satisfy the nonempty production proof. Hardlinks
        // preserve actual wrapper writes even after successful owned-directory cleanup.
        write(&path, b"")?;
        fs::hard_link(path, root.join(format!("observed-{phase}.ok")))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
fn evidence(directory: &Path) -> Value {
    let mut result = serde_json::Map::new();
    for name in [
        "recovery.txt",
        "before.index",
        "before.shared-index",
        "prepared.ok",
        "checkout.ok",
        "committed.ok",
        "checkout-attempted",
        "updates-prepared",
        "updates-committed",
        "updates-aborted",
        "probe-updates-prepared",
        "probe-prepared.ok",
        "probe-committed.ok",
    ] {
        if directory.join(name).is_file() {
            result.insert(name.into(), file(&directory.join(name)));
        }
    }
    Value::Object(result)
}
fn execute_case(root: &Path, case: &str) -> Result<(), String> {
    if case.starts_with("native-") {
        return native_case(root, case);
    }
    if case == "full-fix" {
        return full_fix_case(root);
    }
    if case == "split-index" {
        return split_case(root);
    }
    let refs = if case.starts_with("reftable") {
        "reftable"
    } else {
        "files"
    };
    let objects = if case.ends_with("sha256") {
        "sha256"
    } else {
        "sha1"
    };
    let f = Fixture::new(root, refs, objects)?;
    if case.ends_with("switch-head") || case.starts_with("prelock-") {
        run(&f.repo, &["branch", "other", &f.old], b"", None)?;
    }
    if case == "index-change" {
        let external = root.join("external.index");
        fs::copy(f.repo.join(".git/index"), &external).map_err(|e| e.to_string())?;
        run(
            &f.repo,
            &["update-index", "--assume-unchanged", "file.txt"],
            b"",
            Some(&external),
        )?;
    }
    f.observer(case)?;
    let before = snapshot(&f.repo)?;
    if case == "old-git" || case == "capability-refusal" {
        // The outer fixture supplied the old-Git PATH before this process started.
        let result = Hook::new(&f.repo, "refs/heads/main", &f.old, &f.new);
        let after = snapshot(&f.repo)?;
        let error = result
            .err()
            .ok_or("unsupported Git unexpectedly qualified")?;
        write(
            root.join("case.json"),
            serde_json::to_vec_pretty(
                &json!({"case":case,"before":before,"after":after,"error":error}),
            )
            .map_err(|error| error.to_string())?,
        )?;
        assert_eq!(before, after);
        assert!(error.contains("Git 2.50"), "{error}");
        return Ok(());
    }
    let hook_result = Hook::new(&f.repo, "refs/heads/main", &f.old, &f.new);
    let mut hook = match hook_result {
        Ok(hook) => hook,
        Err(error) => {
            if case == "files-sha1-diagnostic" {
                // Repeat only the owned bare capability transaction, using the existing
                // production bounded transport while retaining normally withheld stderr.
                for entry in fs::read_dir(std::env::temp_dir()).map_err(|e| e.to_string())? {
                    let directory = entry.map_err(|e| e.to_string())?.path();
                    let probe = directory.join("capability.git");
                    if !probe.is_dir() {
                        continue;
                    }
                    let old = run(&probe, &["rev-parse", "HEAD"], b"", None)?;
                    let objects = run(
                        &probe,
                        &[
                            "cat-file",
                            "--batch-all-objects",
                            "--batch-check=%(objectname) %(objecttype)",
                        ],
                        b"",
                        None,
                    )?;
                    let new = objects
                        .lines()
                        .filter_map(|line| line.strip_suffix(" commit"))
                        .find(|oid| *oid != old)
                        .ok_or("missing owned candidate commit")?;
                    let hooks = format!("core.hooksPath={}", directory.display());
                    let input = transaction(&old, new);
                    let output = git::local_repair_input(
                        &f.repo,
                        &[
                            "-C",
                            &probe.to_string_lossy(),
                            "-c",
                            &hooks,
                            "update-ref",
                            "--stdin",
                        ],
                        input.as_bytes(),
                        &[],
                    )?;
                    write(root.join("diagnostic-input"), &input)?;
                    write(root.join("diagnostic.stdout"), &output.stdout)?;
                    write(root.join("diagnostic.stderr"), &output.stderr)?;
                    write(root.join("diagnostic-status"), output.status.to_string())?;
                }
            }
            write(root.join("case.json"),serde_json::to_vec_pretty(&json!({"case":case,"constructor_error":error,"before":before,"after":snapshot(&f.repo)?})).map_err(|error| error.to_string())?)?;
            return Err(format!(
                "supported {refs}/{objects} capability refused: {error}"
            ));
        }
    };
    let directory = hook.directory.clone();
    let capability = evidence(&directory);
    retain_acks(&hook, root)?;
    if case == "missing-ack" {
        hook.capability_acknowledged = false;
    }
    if case == "prelock-switch-head" {
        run(
            &f.repo,
            &[
                "-c",
                "core.hooksPath=",
                "symbolic-ref",
                "HEAD",
                "refs/heads/other",
            ],
            b"",
            None,
        )?;
    } else if case == "prelock-detach-head" {
        run(
            &f.repo,
            &[
                "-c",
                "core.hooksPath=",
                "update-ref",
                "--no-deref",
                "HEAD",
                &f.old,
            ],
            b"",
            None,
        )?;
    }
    let before = if case.starts_with("prelock-") {
        snapshot(&f.repo)?
    } else {
        before
    };
    let held_lock = if case == "held-ref-lock" || case == "checkout-failure" {
        let path = if case == "held-ref-lock" {
            f.repo.join(".git/refs/heads/main.lock")
        } else {
            f.repo.join(".git/index.lock")
        };
        let lock = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|e| e.to_string())?;
        Some(lock)
    } else {
        None
    };
    let started = std::time::Instant::now();
    let result = hook.execute(&f.repo);
    let elapsed = started.elapsed().as_millis();
    let after = snapshot(&f.repo)?;
    let retained = evidence(&directory);
    write(root.join("case.json"),serde_json::to_vec_pretty(&json!({"case":case,"backend":refs,"object_format":objects,"old":f.old,"new":f.new,"before":before,"after":after,"result":result,"elapsed_ms":elapsed,"directory":directory,"directory_retained":directory.exists(),"capability":capability,"evidence":retained})).map_err(|error| error.to_string())?)?;
    drop(held_lock); // Owned file handle only. The owned lock file is deliberately retained.
    if matches!(
        case,
        "files-sha1" | "files-sha256" | "reftable-sha1" | "reftable-sha256"
    ) {
        result?;
        assert_eq!(after["head"], f.new);
        assert_eq!(after["symref"], "refs/heads/main");
        assert_eq!(
            fs::read(f.repo.join("file.txt")).map_err(|error| error.to_string())?,
            b"after\n"
        );
        run(
            &f.repo,
            &["diff-index", "--cached", "--quiet", &f.new, "--"],
            b"",
            None,
        )?;
        for phase in ["prepared", "checkout", "committed"] {
            assert_eq!(
                fs::read(root.join(format!("observed-{phase}.ok")))
                    .map_err(|error| error.to_string())?,
                format!("refs/heads/main\n{}\n{}\n", f.old, f.new).as_bytes()
            );
        }
        let input = fs::read(root.join("original-input-prepared")).map_err(|e| e.to_string())?;
        assert!(
            input
                .split_inclusive(|b| *b == b'\n')
                .any(|line| line == format!("{} {} refs/heads/main\n", f.old, f.new).as_bytes())
        );
        assert_eq!(
            fs::read(root.join("original-args-prepared")).map_err(|error| error.to_string())?,
            b"prepared\0"
        );
        assert_eq!(
            fs::read_to_string(root.join("original-config-prepared"))
                .map_err(|error| error.to_string())?
                .trim(),
            f.root
                .join("original hooks ' \\ path")
                .to_str()
                .ok_or("Owned fixture path is not UTF-8")?
        );
        assert!(!directory.exists());
    } else {
        assert!(result.is_err(), "{case} unexpectedly succeeded");
        if case != "missing-ack" {
            assert!(directory.exists());
            assert!(directory.join("before.index").exists());
            assert!(directory.join("recovery.txt").exists());
        }
        match case {
            "index-change" => {
                assert_eq!(after["head"], before["head"]);
                assert_eq!(
                    fs::read(f.repo.join(".git/index")).map_err(|error| error.to_string())?,
                    fs::read(root.join("external.index")).map_err(|error| error.to_string())?
                );
                assert_eq!(after["work"], before["work"]);
            }
            "work-change" => {
                assert_eq!(after["head"], before["head"]);
                assert_eq!(after["index"], before["index"]);
                assert_eq!(
                    fs::read(f.repo.join("file.txt")).map_err(|error| error.to_string())?,
                    b"external-work\n"
                );
            }
            "committed-failure" => {
                assert_eq!(after["head"], f.new);
                assert_eq!(
                    fs::read(f.repo.join("file.txt")).map_err(|error| error.to_string())?,
                    b"external-after-commit\n"
                );
                assert!(directory.join("checkout.ok").is_file());
            }
            _ => assert_eq!(before, after, "{case} changed repository"),
        }
        if [
            "veto",
            "held-ref-lock",
            "switch-head",
            "detach-head",
            "index-change",
            "work-change",
            "missing-ack",
            "prelock-switch-head",
            "prelock-detach-head",
            "reftable-switch-head",
            "reftable-detach-head",
        ]
        .contains(&case)
        {
            assert!(!directory.join("checkout-attempted").exists());
        }
        if [
            "switch-head",
            "detach-head",
            "reftable-switch-head",
            "reftable-detach-head",
        ]
        .contains(&case)
        {
            assert!(
                fs::read_to_string(root.join("mutation.stderr"))
                    .map_err(|error| error.to_string())?
                    .contains(if case.starts_with("reftable") {
                        "lock"
                    } else {
                        "HEAD.lock"
                    })
            );
        }
        if case.starts_with("prelock-") {
            assert!(!root.join("original-input-prepared").exists());
        }
        if case == "timeout" {
            assert!((14_000..25_000).contains(&elapsed));
        }
    }
    Ok(())
}
