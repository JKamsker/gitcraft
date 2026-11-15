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
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            let mut acts: Vec<Action> = Vec::new();
            tab.sidebar(ui, &mut acts);
            tab.graph_view(ui, &mut acts);
            assert!(acts.is_empty());
        });
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
