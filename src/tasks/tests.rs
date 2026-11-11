include!("tests/local_mutations.rs");

include!("tests/remote_mutations.rs");

#[test]
fn bounds_workers_queue_and_rejects_without_waiting() -> Result<(), String> {
    let pool = Pool::new(2).map_err(|e| e.to_string())?;
    let (started, rx) = mpsc::channel();
    let mut gates = Vec::new();
    for _ in 0..2 {
        let (tx, gate) = mpsc::channel::<()>();
        gates.push(tx);
        let started = started.clone();
        submit(&pool, Priority::Background, None, move || {
            let _ = started.send(());
            let _ = gate.recv();
        })?;
    }
    for _ in 0..2 {
        rx.recv_timeout(Duration::from_secs(5))
            .map_err(|e| e.to_string())?;
    }
    for _ in 0..QUEUE_CAPACITY {
        submit(&pool, Priority::Background, None, || {})?;
    }
    assert_eq!(pool.counts(), (2, QUEUE_CAPACITY));
    assert_eq!(
        submit(&pool, Priority::Interactive, None, || {}),
        Err(BUSY.into())
    );
    pool.shutdown();
    drop(gates);
    Ok(())
}

#[test]
fn reading_ahead_waits_for_all_other_work() -> Result<(), String> {
    let pool = Pool::new(1).map_err(|e| e.to_string())?;
    let (gate_tx, gate) = mpsc::channel::<()>();
    let (started, rx) = mpsc::channel();
    submit(&pool, Priority::Background, None, move || {
        let _ = started.send(());
        let _ = gate.recv();
    })?;
    rx.recv_timeout(Duration::from_secs(5))
        .map_err(|e| e.to_string())?;
    let (out, events) = mpsc::channel();
    for (name, priority) in [
        ("ahead", Priority::Speculative),
        ("background", Priority::Background),
        ("interactive", Priority::Interactive),
    ] {
        let out = out.clone();
        submit(&pool, priority, None, move || {
            let _ = out.send((name, speculative()));
        })?;
    }
    drop(gate_tx);
    let order: Vec<_> = (0..3)
        .map(|_| events.recv_timeout(Duration::from_secs(5)))
        .collect::<Result<_, _>>()
        .map_err(|e| e.to_string())?;
    assert_eq!(
        order,
        [
            ("background", false),
            ("interactive", false),
            ("ahead", true)
        ]
    );
    Ok(())
}

#[test]
fn reads_and_writes_both_make_progress_and_repository_writes_stay_fifo() -> Result<(), String> {
    let pool = Pool::new(1).map_err(|e| e.to_string())?;
    let (gate_tx, gate) = mpsc::channel::<()>();
    let (started, rx) = mpsc::channel();
    submit(&pool, Priority::Background, None, move || {
        let _ = started.send(());
        let _ = gate.recv();
    })?;
    rx.recv_timeout(Duration::from_secs(5))
        .map_err(|e| e.to_string())?;
    let (out, events) = mpsc::channel();
    for (label, priority, key) in [
        ("write 1", Priority::Background, Some("repo")),
        ("write 2", Priority::Interactive, Some("repo")),
        ("read", Priority::Interactive, None),
        ("other repo", Priority::Background, Some("other")),
    ] {
        let out = out.clone();
        submit(&pool, priority, key, move || {
            let _ = out.send(label);
        })?;
    }
    let _ = gate_tx.send(());
    let mut result = Vec::new();
    for _ in 0..4 {
        result.push(
            events
                .recv_timeout(Duration::from_secs(5))
                .map_err(|e| e.to_string())?,
        );
    }
    assert_eq!(result, ["write 1", "write 2", "read", "other repo"]);
    pool.shutdown();
    Ok(())
}

#[test]
fn cancelled_queued_jobs_release_capacity_and_panics_do_not_remove_workers() -> Result<(), String> {
    let pool = Pool::new(1).map_err(|e| e.to_string())?;
    let (gate_tx, gate) = mpsc::channel::<()>();
    let (started, rx) = mpsc::channel();
    submit(&pool, Priority::Background, None, move || {
        let _ = started.send(());
        let _ = gate.recv();
    })?;
    rx.recv_timeout(Duration::from_secs(5))
        .map_err(|e| e.to_string())?;
    let cancel = Cancellation::default();
    let (out, events) = mpsc::channel();
    for _ in 0..QUEUE_CAPACITY {
        let out = out.clone();
        pool.submit(
            cancel.clone(),
            Priority::Interactive,
            None,
            Box::new(move || {
                let _ = out.send("cancelled job ran");
            }),
        )
        .map_err(str::to_owned)?;
    }
    cancel.cancel();
    submit(&pool, Priority::Interactive, None, || {
        std::panic::resume_unwind(Box::new("test panic"))
    })?;
    submit(&pool, Priority::Interactive, None, move || {
        let _ = out.send("worker survived");
    })?;
    let _ = gate_tx.send(());
    assert_eq!(
        events
            .recv_timeout(Duration::from_secs(5))
            .map_err(|e| e.to_string())?,
        "worker survived"
    );
    pool.shutdown();
    Ok(())
}
#[test]
fn cancelling_a_queued_mutation_completes_before_a_worker_becomes_available() -> Result<(), String>
{
    let pool = Pool::new(1).map_err(|e| e.to_string())?;
    let (gate_tx, gate) = mpsc::channel::<()>();
    let (started, rx) = mpsc::channel();
    submit(&pool, Priority::Background, Some("repo"), move || {
        let _ = started.send(());
        let _ = gate.recv();
    })?;
    rx.recv_timeout(Duration::from_secs(5))
        .map_err(|e| e.to_string())?;
    let cancel = Cancellation::default();
    let (done, result) = mpsc::channel();
    let ran = done.clone();
    pool.submit_cancellable(
        cancel.clone(),
        Priority::Background,
        Some("repo".into()),
        Some(Box::new(move || {
            let _ = done.send("cancelled");
        })),
        Box::new(move || {
            let _ = ran.send("ran");
        }),
    )
    .map_err(str::to_owned)?;
    cancel.cancel();
    submit(&pool, Priority::Interactive, None, || {})?;
    assert_eq!(
        result
            .recv_timeout(Duration::from_secs(1))
            .map_err(|e| e.to_string())?,
        "cancelled"
    );
    drop(gate_tx);
    Ok(())
}

#[test]
fn dropping_a_reader_waits_for_its_cancellation_without_cancelling_its_parent() -> Result<(), String>
{
    let parent = Cancellation::default();
    CURRENT.with(|slot| *slot.borrow_mut() = Some(parent.clone()));
    let (started, rx) = mpsc::channel();
    let reader = read(move || {
        let cancel = cancellation();
        let _ = started.send(());
        while !cancel.is_cancelled() {
            std::thread::sleep(Duration::from_millis(5));
        }
    });
    CURRENT.with(|slot| *slot.borrow_mut() = None);
    let reader = reader?;
    rx.recv_timeout(Duration::from_secs(5))
        .map_err(|e| e.to_string())?;
    drop(reader);
    assert!(!parent.is_cancelled());
    Ok(())
}

#[test]
fn urgent_and_speculative_mutations_keep_exclusion_while_reads_progress() -> Result<(), String> {
    for priority in [Priority::Urgent, Priority::Speculative] {
        let pool = Pool::new(3).map_err(|e| e.to_string())?;
        let (release, gate) = mpsc::channel::<()>();
        let (out, events) = mpsc::channel();
        let first = out.clone();
        submit(&pool, Priority::Background, Some("repo-a"), move || {
            let _ = first.send("held");
            let _ = gate.recv();
        })?;
        assert_eq!(
            events
                .recv_timeout(Duration::from_secs(5))
                .map_err(|e| e.to_string())?,
            "held"
        );
        let mutation = out.clone();
        submit(&pool, priority, Some("repo-b"), move || {
            let _ = mutation.send("mutation");
        })?;
        submit(&pool, Priority::Interactive, None, move || {
            let _ = out.send("read");
        })?;
        assert_eq!(
            events
                .recv_timeout(Duration::from_secs(5))
                .map_err(|e| e.to_string())?,
            "read"
        );
        assert!(events.try_recv().is_err());
        drop(release);
        assert_eq!(
            events
                .recv_timeout(Duration::from_secs(5))
                .map_err(|e| e.to_string())?,
            "mutation"
        );
    }
    Ok(())
}

#[test]
fn local_writers_share_a_bucket_while_remote_work_can_run() -> Result<(), String> {
    let pool = Pool::with_limits(3, 16, 1).map_err(|e| e.to_string())?;
    let (gate_tx, gate) = mpsc::channel::<()>();
    let (tx, events) = mpsc::channel();
    let started = tx.clone();
    submit(&pool, Priority::Background, Some("repo"), move || {
        let _ = started.send("write 1");
        let _ = gate.recv();
    })?;
    assert_eq!(
        events
            .recv_timeout(Duration::from_secs(5))
            .map_err(|e| e.to_string())?,
        "write 1"
    );
    let next = tx.clone();
    submit(&pool, Priority::Interactive, Some("repo"), move || {
        let _ = next.send("write 2");
    })?;
    let read = tx.clone();
    submit(
        &pool,
        Priority::Interactive,
        Some("ssh://other/repo"),
        move || {
            let _ = read.send("other repo");
        },
    )?;
    assert_eq!(
        events
            .recv_timeout(Duration::from_secs(5))
            .map_err(|e| e.to_string())?,
        "other repo"
    );
    assert!(
        events.try_recv().is_err(),
        "the second write ran while the first held its key"
    );
    let _ = gate_tx.send(());
    assert_eq!(
        events
            .recv_timeout(Duration::from_secs(5))
            .map_err(|e| e.to_string())?,
        "write 2"
    );
    pool.shutdown();
    Ok(())
}
