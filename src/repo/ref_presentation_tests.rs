//! Display and selection changes must submit no Git/network work.
use super::*;
use crate::git::{Branch, Remote};
use crate::repo::{Action, branch_visibility::Change, palette::Pick};

#[test]
fn presentation_search_and_visibility_preserve_refs_without_submitting_jobs() {
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let mut tab = RepoTab::new_deferred("/nonexistent/233-display".into(), &ctx);
    for alias in ["origin", "alice"] {
        tab.remotes.push(Remote {
            name: alias.into(),
            url: format!("https://example.org/{alias}/repo"),
        });
        tab.refs.remotes.push((
            alias.into(),
            vec![Branch {
                name: format!("{alias}/topic"),
                target: "a".repeat(40),
                is_head: false,
                upstream: String::new(),
                track: String::new(),
            }],
        ));
    }
    let original = tab.refs.clone();
    let (_, jobs) = crate::repo::messages::record_jobs(|| {
        tab.change_visibility(Change::LabelRemote("alice".into()), &ctx);
        assert_eq!(
            tab.ref_labels()["refs/remotes/alice/topic"].caption,
            "topic"
        );
        let choices = tab.palette_choices(&Pick::Checkout);
        assert_eq!(choices.len(), 2);
        assert_ne!(choices[0].key, choices[1].key);
        tab.change_visibility(Change::RemoteSidebar("alice".into()), &ctx);
        assert!(tab.visibility.shows("refs/remotes/alice/topic"));
        assert!(tab.visibility.sidebar_remotes.contains("alice"));
        tab.branch_filter = "topic".into();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let mut acts: Vec<Action> = Vec::new();
            tab.sidebar(ui, &mut acts);
            tab.graph_view(ui, &mut acts);
            assert!(acts.is_empty());
        });
        output.textures_delta.clear();
    });
    assert!(
        jobs.is_empty(),
        "presentation must not submit Git or network commands"
    );
    assert_eq!(tab.refs, original);
    assert_eq!(
        super::super::branch_visibility::Visibility::load(&ctx, tab.repo_key())
            .label_remote
            .name,
        "alice"
    );
}

#[test]
fn large_catalog_is_built_once_per_change_and_lookups_share_the_cache() {
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred("/recording/233-large-catalog".into(), &ctx);
    for alias in ["origin", "alice"] {
        tab.remotes.push(Remote {
            name: alias.into(),
            url: format!("https://example.org/{alias}/repo"),
        });
        tab.refs.remotes.push((
            alias.into(),
            (0..10000)
                .map(|i| Branch {
                    name: format!("{alias}/topic-{i}"),
                    target: "same".into(),
                    is_head: false,
                    upstream: String::new(),
                    track: String::new(),
                })
                .collect(),
        ));
    }
    let first = tab.presentation();
    assert_eq!(first.entries_visited, 20000);
    for i in 0..10000 {
        assert!(std::sync::Arc::ptr_eq(&first, &tab.presentation()));
        assert!(std::sync::Arc::ptr_eq(&first.labels, &tab.ref_labels()));
        assert!(
            tab.ref_description(&format!("refs/remotes/alice/topic-{i}"))
                .contains("https://example.org/alice/repo")
        );
    }
    tab.remotes[1].url = "https://other.example/bob/repo".into();
    tab.rebuild_graph_labels();
    assert!(!std::sync::Arc::ptr_eq(&first, &tab.presentation()));
    assert!(
        tab.ref_description("refs/remotes/alice/topic-0")
            .contains("other.example")
    );
    tab.change_visibility(Change::LabelRemote("alice".into()), &ctx);
    assert_eq!(
        tab.ref_labels()["refs/remotes/alice/topic-0"].caption,
        "topic-0"
    );
    tab.refs.remotes[1].1.pop();
    tab.rebuild_graph_labels();
    assert_eq!(tab.ref_labels().len(), 19999);
}
