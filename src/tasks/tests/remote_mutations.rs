// Remote mutations. Included to preserve libtest shard names.
#[test]
fn remote_mutation_bucket_holds_expectation_and_pop_against_push() -> Result<(), String> {
    // Recording only: no Git commands, remote connection or child processes.
    let pool = Pool::new(2).map_err(|e| e.to_string())?;
    let (release, gate) = mpsc::channel::<()>();
    let (out, events) = mpsc::channel();
    let stash = Arc::new(Mutex::new(vec!["stash A"]));
    let stack = Arc::clone(&stash);
    let record = out.clone();
    pool.submit_cancellable(
        Cancellation::default(),
        Priority::Background,
        Some("ssh://build/main".into()),
        None,
        Box::new(move || {
            assert_eq!(stack.lock().unwrap().last(), Some(&"stash A"));
            let _ = record.send("refs/stash expectation");
            let _ = gate.recv();
            assert_eq!(stack.lock().unwrap().pop(), Some("stash A"));
            let _ = record.send("stash pop --index");
        }),
    )
    .map_err(str::to_owned)?;
    assert_eq!(
        events
            .recv_timeout(Duration::from_secs(5))
            .map_err(|e| e.to_string())?,
        "refs/stash expectation"
    );
    let stack = Arc::clone(&stash);
    let record = out.clone();
    // The other submission API and priority must use the same bucket, even
    // before linked-worktree discovery or through an SSH host alias.
    pool.submit(
        Cancellation::default(),
        Priority::Interactive,
        Some("ssh://alias/linked".into()),
        Box::new(move || {
            stack.lock().unwrap().push("stash B");
            let _ = record.send("stash push");
        }),
    )
    .map_err(str::to_owned)?;
    pool.submit(
        Cancellation::default(),
        Priority::Interactive,
        None,
        Box::new(move || {
            let _ = out.send("concurrent read");
        }),
    )
    .map_err(str::to_owned)?;
    assert_eq!(
        events
            .recv_timeout(Duration::from_secs(5))
            .map_err(|e| e.to_string())?,
        "concurrent read"
    );
    assert!(
        pool.queued_priorities_for(std::path::Path::new("ssh://third/path"))
            == vec![Priority::Interactive]
    );
    drop(release);
    assert_eq!(
        events
            .recv_timeout(Duration::from_secs(5))
            .map_err(|e| e.to_string())?,
        "stash pop --index"
    );
    assert_eq!(
        events
            .recv_timeout(Duration::from_secs(5))
            .map_err(|e| e.to_string())?,
        "stash push"
    );
    assert_eq!(*stash.lock().unwrap(), vec!["stash B"]);
    Ok(())
}

#[test]
fn remote_mutation_classification_matches_execution() {
    for path in [
        "ssh://host/main",
        "SSH://alias/linked",
        r"ssh:\\host\main",
        r"\\wsl.localhost\Ubuntu\repo",
        "ssh:x",
        "/ssh:/repo",
        "/local/common.git",
    ] {
        let path = PathBuf::from(path);
        let expected = if crate::remote::is_remote(&path) {
            MutationScheduleKey::Remote
        } else {
            MutationScheduleKey::Local
        };
        assert_eq!(mutation_schedule_key(path), expected);
    }
    for path in ["ssh:x", "/ssh:/repo", "/local/common.git"] {
        assert_eq!(
            mutation_schedule_key(path.into()),
            MutationScheduleKey::Local
        );
    }
}

#[test]
fn remote_mutation_bucket_preserves_fifo_cancellation_panic_and_local_liveness()
-> Result<(), String> {
    let pool = Pool::new(2).map_err(|e| e.to_string())?;
    let (release, gate) = mpsc::channel::<()>();
    let (out, events) = mpsc::channel();
    let record = out.clone();
    submit(
        &pool,
        Priority::Background,
        Some("ssh://host/main"),
        move || {
            let _ = record.send("first host");
            let _ = gate.recv();
        },
    )?;
    assert_eq!(
        events
            .recv_timeout(Duration::from_secs(5))
            .map_err(|e| e.to_string())?,
        "first host"
    );
    let cancelled = Cancellation::default();
    let (cancel_out, cancel_event) = mpsc::channel();
    pool.submit_cancellable(
        cancelled.clone(),
        Priority::Interactive,
        Some("ssh://alias/linked".into()),
        Some(Box::new(move || {
            let _ = cancel_out.send(());
        })),
        Box::new(|| panic!("cancelled mutation must not run")),
    )
    .map_err(str::to_owned)?;
    cancelled.cancel();
    for (host, label, priority) in [
        ("ssh://second/repo", "second host", Priority::Background),
        ("ssh://third/repo", "third host", Priority::Interactive),
    ] {
        let record = out.clone();
        submit(&pool, priority, Some(host), move || {
            let _ = record.send(label);
        })?;
    }
    submit(
        &pool,
        Priority::Interactive,
        Some("ssh://alias/panic"),
        || {
            std::panic::resume_unwind(Box::new("recorded mutation panic"));
        },
    )?;
    let record = out.clone();
    submit(
        &pool,
        Priority::Background,
        Some("ssh://last/repo"),
        move || {
            let _ = record.send("after panic");
        },
    )?;
    submit(
        &pool,
        Priority::Interactive,
        Some("/local/common.git"),
        move || {
            let _ = out.send("local mutation");
        },
    )?;
    cancel_event
        .recv_timeout(Duration::from_secs(5))
        .map_err(|e| e.to_string())?;
    assert_eq!(
        events
            .recv_timeout(Duration::from_secs(5))
            .map_err(|e| e.to_string())?,
        "local mutation"
    );
    drop(release);
    for expected in ["second host", "third host", "after panic"] {
        assert_eq!(
            events
                .recv_timeout(Duration::from_secs(5))
                .map_err(|e| e.to_string())?,
            expected
        );
    }
    Ok(())
}
