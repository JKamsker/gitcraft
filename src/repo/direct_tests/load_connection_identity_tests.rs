use super::*;
#[test]
fn connection_edits_clear_tab_and_parked_credentials_before_another_action() {
    let a = crate::connections::Context {
        error: Some("profile a".into()),
        ..Default::default()
    };
    let b = crate::connections::Context {
        error: Some("profile b".into()),
        ..Default::default()
    };
    let ctx = egui::Context::default();
    let mut tab = a.run(|| RepoTab::new_deferred("/nonexistent/context-edit-main".into(), &ctx));
    let mut parked =
        a.run(|| RepoTab::new_deferred("/nonexistent/context-edit-worktree".into(), &ctx));
    let creds = git::Credentials {
        scope: None,
        username: "alice".into(),
        password: "never-send-as-bob".into(),
    };
    tab.set_credentials(creds.clone());
    parked.set_credentials(creds);
    tab.parked.push(Box::new(parked));
    b.run(|| tab.notice_connections(&ctx));
    assert!(tab.credentials.is_none());
    assert!(tab.parked[0].credentials.is_none());
    assert_eq!(tab.connection_context, b);
}
