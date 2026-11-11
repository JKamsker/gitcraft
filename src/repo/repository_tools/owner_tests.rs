use super::*;

fn scope(repo: &str, profile: &str) -> Scope {
    Scope {
        path: repo.into(),
        profile: Some(profile.into()),
        epoch: 0,
        connection: Default::default(),
        selected_connection: Default::default(),
        profile_id: 1,
        choices: Default::default(),
        signin: 0,
    }
}
fn request(id: u64, update: Update) -> Request {
    Request {
        ticket: Ticket::new(id, scope("repo", "profile")),
        update,
    }
}
fn receipts() -> Result<Data, String> {
    Ok(Data::StateReceipts(Vec::new()))
}
#[test]
fn constructor_resets_all_transient_inputs() {
    let owner = Owner::new(42, Tool::Refs, "topic".into());
    assert_eq!(owner.id, 42);
    assert_eq!(owner.query, "topic");
    assert!(owner.result.is_none());
    assert!(owner.filter.is_empty() && owner.parent.is_empty());
    assert!(!owner.acknowledged && !owner.compatible && owner.effective);
}
#[test]
fn stale_publication_success_and_error_leave_snapshot_and_consent_untouched() {
    let ticket = request(42, Update::PreparedPublication);
    for current in [scope("other", "profile"), scope("repo", "other")] {
        let mut owner = Owner::new(42, Tool::Publication, String::new());
        owner.acknowledged = true;
        owner.result = Some(Err("current snapshot".into()));
        for result in [receipts(), Err("stale error".into())] {
            assert!(matches!(
                owner.complete(&ticket, &current, result),
                Settlement::None
            ));
            assert!(owner.acknowledged);
            assert!(matches!(&owner.result, Some(Err(e)) if e == "current snapshot"));
        }
    }
}
#[test]
fn accepted_preparation_resets_consent_but_publish_keeps_result_retry_semantics() {
    let mut owner = Owner::new(42, Tool::Publication, String::new());
    for update in [
        Update::PreparedPublication,
        Update::Dependencies,
        Update::Published,
    ] {
        owner.acknowledged = true;
        owner.complete(&request(42, update), &scope("repo", "profile"), receipts());
        assert_eq!(owner.acknowledged, matches!(update, Update::Published));
        assert!(matches!(owner.result, Some(Ok(Data::StateReceipts(_)))));
    }
}
#[test]
fn preview_errors_keep_editor_snapshot_and_only_current_errors_emit_notices() {
    let mut owner = Owner::new(42, Tool::Remote, String::new());
    owner.result = Some(receipts());
    let ticket = request(42, Update::RemotePreview);
    assert!(matches!(
        owner.complete(&ticket, &scope("repo", "other"), Err("old".into())),
        Settlement::None
    ));
    assert!(
        matches!(owner.complete(&ticket, &scope("repo", "profile"), Err("bad".into())), Settlement::Notice(e) if e == "Remote preview: bad")
    );
    assert!(matches!(owner.result, Some(Ok(Data::StateReceipts(_)))));
}
#[test]
fn superseding_request_rejects_old_work_and_teardown_preserves_new_dialog() {
    let id = REQUESTS.fetch_add(1, Ordering::Relaxed);
    let mut owner = Owner::new(id, Tool::Remote, String::new());
    let ticket = Request {
        ticket: Ticket::new(id, scope("repo", "profile")),
        update: Update::RemotePreview,
    };
    let teardown = owner.teardown();
    let _new = owner.restart(scope("repo", "profile"), Update::RemotePreview);
    assert_ne!(owner.id, id);
    assert!(matches!(
        owner.complete(&ticket, &scope("repo", "profile"), receipts()),
        Settlement::None
    ));
    let mut dialog = Some(Dialog::RepositoryTools(Box::new(owner)));
    retire(&mut dialog, teardown);
    assert!(dialog.is_some());
    if let Some(Dialog::RepositoryTools(owner)) = &dialog {
        let teardown = owner.teardown();
        retire(&mut dialog, teardown);
    }
    assert!(dialog.is_none());
}
#[test]
fn read_failures_are_inline_and_subtree_success_clears_search_input() {
    let mut owner = Owner::new(42, Tool::Subtrees, "search".into());
    let ticket = request(42, Update::Read);
    owner.complete(
        &ticket,
        &scope("repo", "profile"),
        Err("read failed".into()),
    );
    assert_eq!(owner.query, "search");
    assert!(matches!(&owner.result, Some(Err(e)) if e == "read failed"));
    owner.complete(&ticket, &scope("repo", "profile"), receipts());
    assert!(owner.query.is_empty());
}
