//! Pure harness tests of the production Owner and tab invalidation adapter.
use super::*;
fn tab() -> RepoTab {
    RepoTab {
        path: "repo-a".into(),
        connection_context: Default::default(),
        https_generation: 0,
        deferred: Default::default(),
        dialog: None,
        outbox_reading: true,
        outbox_last: Some(std::time::Instant::now()),
        busy_op: Some("mutation".into()),
        settled: 0,
    }
}
fn receipts() -> Result<Data, String> {
    Ok(Data::StateReceipts(Vec::new()))
}
#[test]
fn repository_a_b_a_cannot_revive_success_or_error() {
    let ctx = egui::Context;
    for error in [false, true] {
        let mut tab = tab();
        let ticket = tab.prepare_repository_tool(Tool::Refs, &ctx);
        tab.invalidate_repository_tool_requests();
        tab.path = "repo-b".into();
        tab.invalidate_repository_tool_requests();
        tab.path = "repo-a".into();
        let _new = tab.prepare_repository_tool(Tool::Refs, &ctx);
        tab.complete_repository_tool(
            &ticket,
            if error {
                Err("old error".into())
            } else {
                receipts()
            },
            &ctx,
        );
        assert!(matches!(&tab.dialog, Some(Dialog::RepositoryTools(o)) if o.result.is_none()));
        assert_eq!(tab.deferred.epoch(), 2);
    }
}
#[test]
fn alpha_beta_out_of_order_success_and_error_preserve_beta_and_consent() {
    let ctx = egui::Context;
    let mut tab = tab();
    tab.prepare_repository_tool(Tool::Virtuals, &ctx);
    let scope = tab.repository_tool_scope(&ctx);
    let Some(Dialog::RepositoryTools(owner)) = &mut tab.dialog else {
        panic!()
    };
    let alpha = owner.restart(scope.clone(), owner::Update::Prepared);
    let beta = owner.restart(scope, owner::Update::Prepared);
    tab.complete_repository_tool(&beta, receipts(), &ctx);
    let Some(Dialog::RepositoryTools(owner)) = &mut tab.dialog else {
        panic!()
    };
    owner.acknowledged = true;
    for result in [receipts(), Err("alpha failed".into())] {
        tab.complete_repository_tool(&alpha, result, &ctx);
        assert!(matches!(&tab.dialog, Some(Dialog::RepositoryTools(o))
            if o.acknowledged && matches!(o.result, Some(Ok(Data::StateReceipts(_))))));
    }
}
#[test]
fn close_reopen_and_suspend_resume_discard_old_reads() {
    let ctx = egui::Context;
    let mut tab = tab();
    let closed = tab.prepare_repository_tool(Tool::Refs, &ctx);
    tab.dialog = None;
    tab.prepare_repository_tool(Tool::Refs, &ctx);
    tab.complete_repository_tool(&closed, receipts(), &ctx);
    assert!(matches!(&tab.dialog, Some(Dialog::RepositoryTools(o)) if o.result.is_none()));
    let suspended = tab.prepare_repository_tool(Tool::Refs, &ctx);
    tab.invalidate_repository_tool_requests();
    assert!(tab.dialog.is_none());
    assert!(!tab.outbox_reading && tab.outbox_last.is_none());
    assert_eq!(tab.busy_op.as_deref(), Some("mutation"));
    tab.prepare_repository_tool(Tool::Refs, &ctx);
    tab.complete_repository_tool(&suspended, Err("suspended".into()), &ctx);
    assert!(matches!(&tab.dialog, Some(Dialog::RepositoryTools(o)) if o.result.is_none()));
}
#[test]
fn locally_observed_connection_and_signin_changes_advance_epoch() {
    let ctx = egui::Context;
    let mut tab = tab();
    let old = tab.prepare_repository_tool(Tool::Refs, &ctx);
    tab.connection_context.choices.0 = 1;
    tab.repository_tool_scope(&ctx);
    tab.connection_context.choices.0 = 0;
    tab.repository_tool_scope(&ctx);
    assert_eq!(tab.deferred.epoch(), 2);
    tab.prepare_repository_tool(Tool::Refs, &ctx);
    tab.complete_repository_tool(&old, receipts(), &ctx);
    assert!(matches!(&tab.dialog, Some(Dialog::RepositoryTools(o)) if o.result.is_none()));
    tab.https_generation += 1;
    tab.repository_tool_scope(&ctx);
    assert_eq!(tab.deferred.epoch(), 3);
    assert!(tab.dialog.is_none());
}
#[test]
fn epoch_rejects_returned_context_even_when_detached_owner_id_is_unchanged() {
    let ctx = egui::Context;
    let mut tab = tab();
    let read = effects::Read {
        tool: Tool::Outbox,
        query: String::new(),
    };
    let (mut owner, ticket) = Owner::open(&read, tab.repository_tool_scope(&ctx));
    let id = owner.id;
    tab.invalidate_repository_tool_requests();
    tab.path = "repo-b".into();
    tab.invalidate_repository_tool_requests();
    tab.path = "repo-a".into();
    let returned = tab.repository_tool_scope(&ctx);
    for result in [receipts(), Err("old failure".into())] {
        owner.complete(&ticket, &returned, result);
        assert_eq!(owner.id, id);
        assert!(owner.result.is_none());
    }
}
