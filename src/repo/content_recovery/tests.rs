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
            root_pin: None,
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

#[test]
fn pointer_ui_roundtrip_searches_inspects_prepares_and_recovers_a_new_file() -> Result<(), String> {
    struct Scratch(std::path::PathBuf);
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    static ID: AtomicU64 = AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "gitcraft-content-ui-{}-{}",
        std::process::id(),
        ID.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&path).map_err(|e| e.to_string())?;
    let _scratch = Scratch(path.clone());
    for args in [
        vec!["init", "-q", "-b", "main"],
        vec!["config", "user.name", "Test"],
        vec!["config", "user.email", "test@example.com"],
        vec!["config", "commit.gpgSign", "false"],
    ] {
        crate::git::run(Some(&path), &args)?;
    }
    std::fs::write(path.join("file"), b"parse_header restored bytes\n")
        .map_err(|e| e.to_string())?;
    crate::git::run(Some(&path), &["add", "file"])?;
    crate::git::run(Some(&path), &["commit", "-qm", "source"])?;
    let index = std::fs::read(path.join(".git/index")).map_err(|e| e.to_string())?;
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let mut tab = tab(&ctx);
    tab.path = path.clone();
    fn frame(
        tab: &mut RepoTab,
        ctx: &egui::Context,
        time: &mut f64,
        events: Vec<egui::Event>,
    ) -> Vec<egui::epaint::ClippedShape> {
        *time += 0.05;
        let mut actions = Vec::new();
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1200.0, 1000.0),
                )),
                time: Some(*time),
                events,
                ..Default::default()
            },
            |ui| tab.dialogs(ui.ctx(), &mut actions),
        );
        output.textures_delta.clear();
        let shapes = std::mem::take(&mut output.shapes);
        for action in actions {
            tab.apply(action, ctx);
        }
        shapes
    }
    fn label(shape: &egui::epaint::Shape, needle: &str) -> Option<egui::Pos2> {
        match shape {
            egui::epaint::Shape::Text(text) if text.galley.job.text.contains(needle) => {
                Some(text.visual_bounding_rect().center())
            }
            egui::epaint::Shape::Vec(shapes) => {
                shapes.iter().find_map(|shape| label(shape, needle))
            }
            _ => None,
        }
    }
    fn point(shapes: &[egui::epaint::ClippedShape], needle: &str) -> Result<egui::Pos2, String> {
        shapes
            .iter()
            .find_map(|shape| label(&shape.shape, needle))
            .ok_or_else(|| format!("UI label was not rendered: {needle}"))
    }
    fn click(tab: &mut RepoTab, ctx: &egui::Context, time: &mut f64, pos: egui::Pos2) {
        for pressed in [true, false] {
            frame(
                tab,
                ctx,
                time,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
    }
    let mut time = 0.0;
    let shapes = (0..3)
        .map(|_| frame(&mut tab, &ctx, &mut time, vec![]))
        .last()
        .unwrap_or_default();
    click(
        &mut tab,
        &ctx,
        &mut time,
        point(&shapes, "Text you remember")?,
    );
    frame(
        &mut tab,
        &ctx,
        &mut time,
        vec![egui::Event::Text("parse_header".into())],
    );
    assert_eq!(tab.content_recovery.query, "parse_header");
    let shapes = (0..3)
        .map(|_| frame(&mut tab, &ctx, &mut time, vec![]))
        .last()
        .unwrap_or_default();
    click(
        &mut tab,
        &ctx,
        &mut time,
        point(&shapes, "Search literal text")?,
    );
    finish(&mut tab, &ctx)?;
    assert!(
        tab.content_recovery
            .results
            .as_ref()
            .is_some_and(|results| !results.report.hits.is_empty())
    );
    let shapes = (0..3)
        .map(|_| frame(&mut tab, &ctx, &mut time, vec![]))
        .last()
        .unwrap_or_default();
    click(&mut tab, &ctx, &mut time, point(&shapes, " · file · ")?);
    finish(&mut tab, &ctx)?;
    assert!(tab.content_recovery.preview.is_some());
    let destination = tab.content_recovery.file.clone();
    let shapes = (0..3)
        .map(|_| frame(&mut tab, &ctx, &mut time, vec![]))
        .last()
        .unwrap_or_default();
    click(
        &mut tab,
        &ctx,
        &mut time,
        point(&shapes, "Preview file recovery")?,
    );
    finish(&mut tab, &ctx)?;
    assert!(tab.content_recovery.prepared.is_some());
    assert!(!path.join(&destination).exists());
    let shapes = (0..3)
        .map(|_| frame(&mut tab, &ctx, &mut time, vec![]))
        .last()
        .unwrap_or_default();
    click(&mut tab, &ctx, &mut time, point(&shapes, "Recover now")?);
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let msg = tab
            .rx
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .map_err(|e| e.to_string())?;
        if let Msg::ContentRecovered(result) = msg {
            assert!(result.is_ok(), "{result:?}");
            tab.content_recovered(result, &ctx);
            break;
        }
    }
    assert_eq!(
        std::fs::read(path.join(destination)).map_err(|e| e.to_string())?,
        b"parse_header restored bytes\n"
    );
    assert_eq!(
        std::fs::read(path.join(".git/index")).map_err(|e| e.to_string())?,
        index
    );
    assert!(tab.busy_op.is_none());
    // Complete the ordinary refresh before this owned fixture is removed.
    while tab.loading {
        tab.poll(&ctx);
        if Instant::now() >= deadline {
            return Err("Recovery refresh did not finish".into());
        }
        std::thread::yield_now();
    }
    Ok(())
}
