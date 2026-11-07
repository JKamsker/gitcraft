use super::*;
use std::sync::mpsc;
fn tab(ctx: &egui::Context) -> RepoTab {
    let mut tab = RepoTab::new_deferred("/nonexistent/content-reader-fixture".into(), ctx);
    let (tx, rx) = mpsc::channel();
    tab.tx = tx;
    tab.rx = rx;
    tab.dialog = Some(Dialog::ContentRecovery);
    tab
}
fn result(label: &str) -> Read {
    Read::Results(Arc::new(engine::Results {
        identity: engine::Identity {
            host: None,
            common: "/recording/.git".into(),
            root: Some("/recording".into()),
            format: "sha1".into(),
        },
        query: label.into(),
        report: Default::default(),
    }))
}
fn finish(tab: &mut RepoTab, ctx: &egui::Context) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let msg = tab
            .rx
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .map_err(|e| e.to_string())?;
        if let Msg::ContentRead(id, result) = msg {
            tab.content_read(id, result, ctx);
            return Ok(());
        }
    }
}
#[test]
fn replacements_wait_for_the_actual_owned_read_to_return_and_only_start_the_latest()
-> Result<(), String> {
    use std::sync::atomic::AtomicUsize;
    let ctx = egui::Context::default();
    let mut tab = tab(&ctx);
    let (entered, entry) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let (check, checked) = mpsc::channel();
    let (state, states) = mpsc::channel();
    tab.content_recovery
        .enqueue(Job::Inject(Box::new(move |active| {
            entered.send(()).map_err(|e| e.to_string())?;
            checked
                .recv_timeout(Duration::from_secs(5))
                .map_err(|e| e.to_string())?;
            state.send(active()).map_err(|e| e.to_string())?;
            released
                .recv_timeout(Duration::from_secs(5))
                .map_err(|e| e.to_string())?;
            Ok(result("obsolete"))
        })));
    tab.poll_content_recovery(&ctx);
    entry
        .recv_timeout(Duration::from_secs(5))
        .map_err(|e| e.to_string())?;
    let started = Arc::new(AtomicUsize::new(0));
    for n in 0..100 {
        let started = started.clone();
        tab.content_recovery
            .enqueue(Job::Inject(Box::new(move |active| {
                assert!(active());
                started.fetch_add(1, Ordering::Relaxed);
                Ok(result(&format!("latest {n}")))
            })));
        tab.poll_content_recovery(&ctx);
    }
    check.send(()).map_err(|e| e.to_string())?;
    assert!(
        !states
            .recv_timeout(Duration::from_secs(5))
            .map_err(|e| e.to_string())?
    );
    assert_eq!(started.load(Ordering::Relaxed), 0);
    // A caller timeout/cancel event does not release ownership. Only this actual
    // worker completion lets the queued next request start; there is no timer shortcut.
    release.send(()).map_err(|e| e.to_string())?;
    finish(&mut tab, &ctx)?;
    assert!(tab.content_recovery.results.is_none());
    finish(&mut tab, &ctx)?;
    assert_eq!(started.load(Ordering::Relaxed), 1);
    assert_eq!(
        tab.content_recovery
            .results
            .as_ref()
            .map(|r| r.query.as_str()),
        Some("latest 99")
    );
    Ok(())
}
#[test]
fn escape_closes_and_invalidates_a_live_content_read_without_closing_the_tab() -> Result<(), String>
{
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let mut tab = tab(&ctx);
    let (entered, entry) = mpsc::channel();
    let (check, checked) = mpsc::channel();
    let (state, states) = mpsc::channel();
    tab.content_recovery
        .enqueue(Job::Inject(Box::new(move |active| {
            entered.send(()).map_err(|e| e.to_string())?;
            checked
                .recv_timeout(Duration::from_secs(5))
                .map_err(|e| e.to_string())?;
            state.send(active()).map_err(|e| e.to_string())?;
            Ok(result("old"))
        })));
    tab.poll_content_recovery(&ctx);
    entry
        .recv_timeout(Duration::from_secs(5))
        .map_err(|e| e.to_string())?;
    let mut frame = ctx.run_ui(Default::default(), |ui| {
        tab.dialogs(ui.ctx(), &mut Vec::new())
    });
    frame.textures_delta.clear();
    let input = egui::RawInput {
        events: vec![egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
        ..Default::default()
    };
    let mut frame = ctx.run_ui(input, |ui| tab.dialogs(ui.ctx(), &mut Vec::new()));
    frame.textures_delta.clear();
    assert!(tab.dialog.is_none());
    check.send(()).map_err(|e| e.to_string())?;
    assert!(
        !states
            .recv_timeout(Duration::from_secs(5))
            .map_err(|e| e.to_string())?
    );
    finish(&mut tab, &ctx)?;
    assert!(tab.content_recovery.results.is_none());
    assert!(!tab.content_recovery.loading());
    Ok(())
}
#[test]
fn pending_read_preserves_the_initiating_profile_scope_without_any_remote_execution()
-> Result<(), String> {
    let ctx = egui::Context::default();
    let mut tab = tab(&ctx);
    let profile = crate::connections::Context {
        choices: crate::connections::Choices {
            ssh_key: "/recording/profile-key".into(),
            ..Default::default()
        },
        ..Default::default()
    };
    let expected = profile.clone();
    profile.run(|| {
        tab.content_recovery.enqueue(Job::Inject(Box::new(move |_| {
            assert_eq!(crate::connections::current(), expected);
            Ok(result("profile"))
        })))
    });
    tab.poll_content_recovery(&ctx);
    finish(&mut tab, &ctx)?;
    assert_eq!(
        tab.content_recovery
            .results
            .as_ref()
            .map(|r| r.query.as_str()),
        Some("profile")
    );
    Ok(())
}
