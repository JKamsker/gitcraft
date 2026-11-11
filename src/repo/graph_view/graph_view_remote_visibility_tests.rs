use super::*;
#[test]
fn hidden_remote_pills_disappear_but_local_head_is_kept() {
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred("/nonexistent/remote-pills".into(), &ctx);
    tab.refs.current = Some("main".into());
    tab.visibility.toggle_remote_graph("origin");
    tab.visibility.toggle_remote_graph("fork");
    let commit = git::Commit {
        refs: [
            "HEAD -> main",
            "refs/remotes/origin/main",
            "refs/remotes/fork/topic",
        ]
        .map(String::from)
        .to_vec(),
        ..Default::default()
    };
    let badges = tab.graph_badges(&commit);
    assert_eq!(badges.len(), 1);
    assert_eq!(badges[0].label, "main");
    assert!(badges[0].head && badges[0].local);
    assert!(badges[0].remotes.is_empty());
    tab.visibility.toggle_solo("refs/remotes/fork/topic");
    assert_eq!(tab.graph_badges(&commit).len(), 2);
}
