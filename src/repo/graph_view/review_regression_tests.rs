//! Collision regressions from the independent review of 8b1ed1df.
use super::*;
use std::sync::Arc;

fn branch(name: &str, upstream: &str) -> git::Branch {
    git::Branch {
        name: name.into(),
        target: "tip".into(),
        is_head: false,
        upstream: upstream.into(),
        track: String::new(),
    }
}

#[test]
fn comparison_focus_and_row_comparison_keep_full_remote_identity() -> Result<(), String> {
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred("/recording/233-focus".into(), &ctx);
    tab.refs.local = vec![branch("origin/topic", "")];
    tab.refs.remotes = vec![("origin".into(), vec![branch("origin/topic", "")])];
    tab.commits = vec![
        Arc::new(git::Commit {
            hash: "tip".into(),
            refs: vec![
                "refs/heads/origin/topic".into(),
                "refs/remotes/origin/topic".into(),
            ],
            ..Default::default()
        }),
        Arc::new(git::Commit {
            hash: "base".into(),
            ..Default::default()
        }),
    ];
    tab.rebuild_graph_labels();
    let action = crate::repo::actions::compare_branches_action(
        BranchRef::local("base"),
        BranchRef::remote("origin/topic"),
        false,
    );
    let Action::Compare {
        base,
        base_label,
        target,
        target_label,
        captions,
        since_fork,
    } = action
    else {
        return Err("comparison required".into());
    };
    tab.compare = Some(crate::repo::Compare {
        base,
        base_label,
        target,
        target_label,
        captions,
        since_fork,
        data: None,
        request: 0,
    });
    let badges = tab.graph_badges(&tab.commits[0]);
    assert_eq!(badges[0].full_ref, "refs/remotes/origin/topic");
    assert!(
        matches!(tab.compare_rows(0, 1), Some(Action::Compare { target: Some(full), target_label, .. })
        if full == "refs/remotes/origin/topic" && target_label == "topic")
    );
    Ok(())
}
