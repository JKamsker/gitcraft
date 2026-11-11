use super::*;
#[test]
fn destination_capture_runs_only_in_the_owned_export_worker() -> Result<(), String> {
    use std::{
        sync::mpsc,
        time::{Duration, Instant},
    };
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred("/nonexistent/export-capture".into(), &ctx);
    let ui_thread = std::thread::current().id();
    let (captured, received) = mpsc::channel();
    let path = std::path::PathBuf::from("/nonexistent/gitcraft-postcard-parent/output.png");
    tab.queue_postcard_export(path, Arc::new(Vec::new()), move |path| {
        assert_ne!(std::thread::current().id(), ui_thread);
        captured.send(()).map_err(|e| e.to_string())?;
        export::Destination::capture(path)
    });
    assert!(received.try_recv().is_err());
    assert!(tab.postcards.reader.poll(&tab.tx, &ctx).is_none());
    received
        .recv_timeout(Duration::from_secs(5))
        .map_err(|e| e.to_string())?;
    let start = Instant::now();
    loop {
        if let Some(result) = tab.postcards.reader.poll(&tab.tx, &ctx) {
            assert!(result.is_err());
            break;
        }
        if start.elapsed() > Duration::from_secs(5) {
            return Err("Export did not finish".into());
        }
        std::thread::yield_now();
    }
    Ok(())
}
#[test]
fn shortened_contributor_names_keep_distinct_disclosed_identities() {
    let prefix = "A".repeat(300);
    let commits = vec![
        Commit {
            author: format!("{prefix}one"),
            ..Default::default()
        },
        Commit {
            author: format!("{prefix}two"),
            ..Default::default()
        },
    ];
    let snapshot = Snapshot::new(
        &State {
            include_names: true,
            ..Default::default()
        },
        &commits,
    );
    assert_eq!(snapshot.contributors.len(), 2);
    assert_ne!(snapshot.contributors[0], snapshot.contributors[1]);
    assert!(
        snapshot
            .contributors
            .iter()
            .all(|n| n.contains('…') && n.len() < 300)
    );
    assert_eq!(clean("abcd", 3), "abc…");
}
#[test]
fn closing_discards_owned_disclosures_before_a_later_profile_reopens() {
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred("/nonexistent/postcard-closed-owner".into(), &ctx);
    tab.postcards.commits = Some(vec![Commit {
        author: "Old private contributor".into(),
        ..Default::default()
    }]);
    tab.postcards.context = Some(crate::git::profile_config());
    tab.postcards.open = false;
    tab.poll_postcards(&ctx);
    assert!(tab.postcards.commits.is_none());
    assert!(tab.postcards.preview.is_none());
    tab.postcards.open = true;
    tab.postcards.context = Some(Some("/different/profile".into()));
    tab.poll_postcards(&ctx);
    assert!(tab.postcards.commits.is_none());
}
#[test]
fn profile_change_cancels_a_queued_read_and_discards_prior_disclosures() {
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred("/nonexistent/postcard-profile-fixture".into(), &ctx);
    tab.postcards.open = true;
    tab.postcards.commits = Some(vec![Commit {
        author: "Old profile contributor".into(),
        ..Default::default()
    }]);
    tab.postcards.context = Some(Some("/different/profile".into()));
    tab.postcards
        .reader
        .queue(|_| Err("A cancelled queued read must not start".into()));
    tab.poll_postcards(&ctx);
    assert!(!tab.postcards.reader.busy());
    assert!(tab.postcards.commits.is_none());
    assert!(tab.postcards.preview.is_none());
    assert!(
        tab.postcards
            .status
            .as_deref()
            .is_some_and(|status| status.contains("Profile changed"))
    );
}
#[test]
fn anonymization_removes_names_and_subjects_from_render_input_and_preview() {
    let mut state = State {
        anonymize: true,
        include_names: true,
        target: "a".repeat(40),
        ..Default::default()
    };
    let commits = vec![Commit {
        hash: "a".repeat(40),
        author: "Private Person".into(),
        email: "secret@example.com".into(),
        summary: "Private release message".into(),
        body: "Private body".into(),
        ..Default::default()
    }];
    let snapshot = Snapshot::new(&state, &commits);
    assert_eq!(snapshot.messages, vec!["Commit 1"]);
    assert_eq!(snapshot.contributors, vec!["Contributor 1"]);
    assert!(
        snapshot.commits[0].author.is_empty()
            && snapshot.commits[0].email.is_empty()
            && snapshot.commits[0].summary.is_empty()
            && snapshot.commits[0].body.is_empty()
    );
    state.anonymize = false;
    let snapshot = Snapshot::new(&state, &commits);
    assert_eq!(snapshot.messages, vec!["Private release message"]);
    assert_eq!(snapshot.contributors, vec!["Private Person"]);
}
#[test]
fn composed_png_decodes_at_expected_size_and_contains_painted_graph_and_text() -> Result<(), String>
{
    let state = State {
        name: "Release 1.0".into(),
        date: "2026-11-07".into(),
        target: "a".repeat(40),
        anonymize: true,
        ..Default::default()
    };
    let snapshot = Snapshot::new(
        &state,
        &[Commit {
            hash: "a".repeat(40),
            ..Default::default()
        }],
    );
    let (image, png) = render::render(&snapshot, &|| true)?;
    let decoded = image::load_from_memory(&png).map_err(|e| e.to_string())?;
    assert_eq!(decoded.width(), image.size[0] as u32);
    assert_eq!(decoded.height(), image.size[1] as u32);
    let bg = image.pixels[0];
    assert!(image.pixels.iter().filter(|p| **p != bg).count() > 400);
    assert!(render::render(&snapshot, &|| false).is_err());
    Ok(())
}
#[test]
fn postcard_controls_anonymize_then_compose_the_displayed_preview() -> Result<(), String> {
    use egui_kittest::{Harness, kittest::Queryable};
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred("/nonexistent/postcard-ui".into(), &ctx);
    tab.postcards = State {
        open: true,
        name: "Release".into(),
        date: "2026-11-07".into(),
        target: "a".repeat(40),
        include_names: true,
        commits: Some(vec![Commit {
            hash: "a".repeat(40),
            author: "Private Person".into(),
            summary: "Private subject".into(),
            ..Default::default()
        }]),
        ..Default::default()
    };
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1200.0, 1000.0))
        .build_ui_state(|ui, tab: &mut RepoTab| tab.postcard_window(ui.ctx()), tab);
    harness.run();
    harness
        .get_by_label("Anonymize names and replace all subjects with commit numbers")
        .click();
    harness.run();
    assert!(harness.state().postcards.anonymize);
    harness.get_by_label("Compose PNG preview").click();
    harness.run_steps(2);
    let began = std::time::Instant::now();
    while harness.state().postcards.preview.is_none() {
        if began.elapsed() > std::time::Duration::from_secs(4) {
            return Err(format!(
                "Preview did not finish: {:?}",
                harness.state().postcards.status
            ));
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
        harness.run_steps(1);
    }
    let preview = harness
        .state()
        .postcards
        .preview
        .as_ref()
        .ok_or("preview")?;
    assert_eq!(preview.snapshot.messages, vec!["Commit 1"]);
    assert_eq!(preview.snapshot.contributors, vec!["Contributor 1"]);
    assert!(preview.png.starts_with(b"\x89PNG"));
    Ok(())
}
