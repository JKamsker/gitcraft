use super::*;
#[test]
fn only_new_matching_lines_get_personal_cited_observations() -> Result<(), String> {
    let comment = Comment {
        url: "https://forge/review/1".into(),
        body: "No unwrap in handlers".into(),
        author: "me".into(),
    };
    let book = Book {
        enabled: true,
        comments: vec![comment],
        rules: vec![Rule {
            summary: "avoid unwrap".into(),
            pattern: r"\.unwrap\(\)".into(),
            source: "https://forge/review/1".into(),
            enabled: true,
        }],
    };
    let hits = check_patch(
        "+++ b/handler.rs\n@@ -1,2 +1,2 @@\n-old.unwrap()\n+new.unwrap()\n context.unwrap()",
        &book,
    )?;
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].line, 1);
    assert_eq!(hits[0].path, "handler.rs");
    let disguised = check_patch(
        "diff --git a/real.rs b/real.rs\n--- a/real.rs\n+++ b/real.rs\n@@ -0,0 +1 @@\n+++ b/fake.unwrap()\ndiff --git a/next.rs b/next.rs\n--- a/next.rs\n+++ b/next.rs\n@@ -0,0 +1 @@\n+next.unwrap()",
        &book,
    )?;
    assert_eq!(disguised.len(), 2);
    assert_eq!(disguised[0].path, "real.rs");
    assert_eq!(disguised[1].path, "next.rs");
    let quoted = check_patch(
        "+++ \"b/quoted\\tname.rs\"\n@@ -0,0 +1 @@\n+x.unwrap()",
        &book,
    )?;
    assert_eq!(quoted[0].path, "quoted\tname.rs");
    let mut forged = book.clone();
    forged.rules[0].source = "https://invented".into();
    assert!(check_patch("", &forged).is_err());
    forged.enabled = false;
    assert!(check_patch("+new.unwrap()", &forged)?.is_empty());
    Ok(())
}
#[test]
fn ordinary_preparation_keeps_queue_until_echo_and_push_complete() -> Result<(), String> {
    let repo = git::stack::test_repo::TestRepo::new("echo-prepared-queue")?;
    let bare = repo.path().join("remote.git");
    repo.git(&["init", "--bare", bare.to_str().ok_or("path")?])?;
    repo.git(&["remote", "add", "origin", bare.to_str().ok_or("path")?])?;
    repo.git(&["push", "-u", "origin", "main"])?;
    let head = repo.commit("later", "outgoing\n", "outgoing")?;
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(repo.path().to_owned(), &ctx);
    tab.start_deferred(&ctx);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while tab.opening() || tab.loading {
        tab.poll(&ctx);
        if std::time::Instant::now() > deadline {
            return Err("opening timed out".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    // Force the persisted-preference lookup without writing a shared personal rulebook.
    tab.review_tools.echo.book.enabled = true;
    tab.apply(Action::Push, &ctx);
    tab.apply(Action::Fetch, &ctx);
    assert_eq!(tab.waiting.len(), 1);
    // Process only the preparation handoff, leaving echo's asynchronous result queued.
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        match tab.rx.recv_timeout(remaining).map_err(|e| e.to_string())? {
            crate::repo::Msg::Then(f) => f(&mut tab, &ctx),
            _ => continue,
        }
        if tab.review_tools.echo.checking_push {
            break;
        }
    }
    assert!(tab.busy_op.is_none());
    assert_eq!(
        tab.waiting.len(),
        1,
        "echo is still holding the queued fetch"
    );
    loop {
        tab.poll(&ctx);
        if tab.busy_op.is_none()
            && !tab.loading
            && !tab.echo_push_pending()
            && tab.waiting.is_empty()
        {
            break;
        }
        if std::time::Instant::now() > deadline {
            return Err(format!("push/queue timed out: {:?}", tab.status));
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(git::resolve(&bare, "refs/heads/main")?, head);
    assert!(
        tab.activity
            .last()
            .is_some_and(|entry| entry.label == "Fetch"),
        "queued fetch ran after push"
    );
    Ok(())
}

#[test]
fn echo_cancel_and_stale_resume_settle_the_waiting_queue() -> Result<(), String> {
    let repo = git::stack::test_repo::TestRepo::new("echo-queue-cancel")?;
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(repo.path().to_owned(), &ctx);
    let waiting = || Waiting {
        continuation: None,
        op: PendingOp {
            label: "Push".into(),
            ops: vec![vec!["push".into()]],
            refresh: crate::repo::Refresh::All,
            kind: crate::repo::OpKind::plain(),
            quiet: false,
        },
        stamp: "stale-stamp".into(),
        source: None,
    };
    tab.review_tools.echo.pending_push = Some(waiting());
    tab.waiting.push_back(crate::repo::running::Waiter::Fetch);
    // An unrelated completion cannot drain or discard the queue owned by the held push.
    tab.after_op(true, true, &ctx);
    assert_eq!(tab.waiting.len(), 1);
    tab.after_op(false, false, &ctx);
    assert_eq!(tab.waiting.len(), 1);
    assert!(tab.busy_op.is_none());
    tab.echo_act(Act::CancelPush, &ctx);
    assert!(tab.waiting.is_empty());
    assert!(!tab.echo_push_pending());
    tab.review_tools.echo.pending_push = Some(waiting());
    tab.waiting.push_back(crate::repo::running::Waiter::Fetch);
    tab.resume_echo_push(&ctx);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while tab.review_tools.echo.checking_push {
        tab.poll(&ctx);
        if std::time::Instant::now() > deadline {
            return Err("stale echo timed out".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(tab.waiting.is_empty());
    assert!(
        tab.review_tools
            .echo
            .error
            .as_deref()
            .is_some_and(|e| e.contains("changed after reviewer echo"))
    );
    Ok(())
}
