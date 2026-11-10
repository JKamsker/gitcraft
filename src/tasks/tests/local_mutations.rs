// Local mutations. Included to preserve libtest shard names.
use super::*;

use std::sync::mpsc;

use std::time::Duration;

#[test]
fn local_metadata_aliases_hold_expectation_and_pop_against_alias_push() -> Result<(), String> {
    // Pure classification cannot invoke filesystem lookup, even for paths that
    // are missing, inaccessible, aliased or replaced after submission.
    for path in [
        "/recorded/mount-a/common.git",
        "/recorded/mount-b/common.git",
        "/recorded/independent/common.git",
        "/missing/common.git",
    ] {
        assert_eq!(
            mutation_schedule_key(path.into()),
            MutationScheduleKey::Local
        );
    }
    let pool = Pool::new(3).map_err(|e| e.to_string())?;
    let (release, gate) = mpsc::channel::<()>();
    let (out, events) = mpsc::channel();
    let stash = Arc::new(Mutex::new(vec!["stash A"]));
    let stack = Arc::clone(&stash);
    let record = out.clone();
    pool.submit_cancellable(
        Cancellation::default(),
        Priority::Background,
        Some("/recorded/mount-a/common.git".into()),
        None,
        Box::new(move || {
            assert_eq!(stack.lock().unwrap().last(), Some(&"stash A"));
            let _ = record.send("expect A");
            let _ = gate.recv();
            assert_eq!(stack.lock().unwrap().pop(), Some("stash A"));
            let _ = record.send("pop A");
        }),
    )
    .map_err(str::to_owned)?;
    assert_eq!(
        events
            .recv_timeout(Duration::from_secs(5))
            .map_err(|e| e.to_string())?,
        "expect A"
    );
    let queued = Cancellation::default();
    let stack = Arc::clone(&stash);
    let record = out.clone();
    pool.submit(
        queued.clone(),
        Priority::Interactive,
        Some("/recorded/mount-b/common.git".into()),
        Box::new(move || {
            stack.lock().unwrap().push("stash B");
            let _ = record.send("push B");
        }),
    )
    .map_err(str::to_owned)?;
    assert!(pool.queued_priority_for(&queued) == Some(Priority::Interactive));
    for (key, event) in [
        (None, "read"),
        (Some("ssh://independent/repo".into()), "remote"),
    ] {
        let record = out.clone();
        pool.submit(
            Cancellation::default(),
            Priority::Interactive,
            key,
            Box::new(move || {
                let _ = record.send(event);
            }),
        )
        .map_err(str::to_owned)?;
    }
    let progress = [
        events
            .recv_timeout(Duration::from_secs(5))
            .map_err(|e| e.to_string())?,
        events
            .recv_timeout(Duration::from_secs(5))
            .map_err(|e| e.to_string())?,
    ];
    assert!(progress.contains(&"read") && progress.contains(&"remote"));
    assert!(pool.queued_priority_for(&queued).is_some());
    drop(release);
    assert_eq!(
        events
            .recv_timeout(Duration::from_secs(5))
            .map_err(|e| e.to_string())?,
        "pop A"
    );
    assert_eq!(
        events
            .recv_timeout(Duration::from_secs(5))
            .map_err(|e| e.to_string())?,
        "push B"
    );
    assert_eq!(*stash.lock().unwrap(), ["stash B"]);
    assert_eq!(
        mutation_schedule_key("ssh://unopened/repo".into()),
        MutationScheduleKey::Remote
    );
    Ok(())
}

#[test]
fn local_retargeting_and_running_cancellation_keep_the_bucket_until_completion()
-> Result<(), String> {
    let pool = Pool::new(3).map_err(|e| e.to_string())?;
    let (release, gate) = mpsc::channel::<()>();
    let (out, events) = mpsc::channel();
    // Routing is observed at execution, just as Git resolves captured paths then.
    let route = Arc::new(Mutex::new("old metadata"));
    let target = Arc::clone(&route);
    let record = out.clone();
    let running = Cancellation::default();
    pool.submit(
        running.clone(),
        Priority::Background,
        Some("/repo/a".into()),
        Box::new(move || {
            let _ = record.send("expect A");
            let _ = gate.recv();
            assert_eq!(*target.lock().unwrap(), "replacement metadata");
            let _ = record.send("pop A");
        }),
    )
    .map_err(str::to_owned)?;
    assert_eq!(
        events
            .recv_timeout(Duration::from_secs(5))
            .map_err(|e| e.to_string())?,
        "expect A"
    );
    *route.lock().unwrap() = "replacement metadata";
    running.cancel(); // Cancellation is a request, not completion of the closure.
    let competing = Cancellation::default();
    let record = out.clone();
    let target = Arc::clone(&route);
    pool.submit_cancellable(
        competing.clone(),
        Priority::Interactive,
        Some("/repo/b".into()),
        None,
        Box::new(move || {
            assert_eq!(*target.lock().unwrap(), "replacement metadata");
            let _ = record.send("push B");
        }),
    )
    .map_err(str::to_owned)?;
    let independent = Cancellation::default();
    let record = out.clone();
    pool.submit(
        independent.clone(),
        Priority::Background,
        Some("/independent".into()),
        Box::new(move || {
            let _ = record.send("independent local");
        }),
    )
    .map_err(str::to_owned)?;
    let record = out.clone();
    pool.submit(
        Cancellation::default(),
        Priority::Interactive,
        None,
        Box::new(move || {
            let _ = record.send("read");
        }),
    )
    .map_err(str::to_owned)?;
    assert_eq!(
        events
            .recv_timeout(Duration::from_secs(5))
            .map_err(|e| e.to_string())?,
        "read"
    );
    assert!(pool.queued_priority_for(&competing).is_some());
    assert!(pool.queued_priority_for(&independent).is_some());
    drop(release);
    for expected in ["pop A", "push B", "independent local"] {
        assert_eq!(
            events
                .recv_timeout(Duration::from_secs(5))
                .map_err(|e| e.to_string())?,
            expected
        );
    }
    Ok(())
}

fn submit(
    pool: &Pool,
    priority: Priority,
    key: Option<&str>,
    work: impl FnOnce() + Send + 'static,
) -> Result<(), String> {
    pool.submit(
        Cancellation::default(),
        priority,
        key.map(PathBuf::from),
        Box::new(work),
    )
    .map_err(str::to_owned)
}
