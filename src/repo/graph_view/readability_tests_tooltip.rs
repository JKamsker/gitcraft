#[test]
fn real_git_tooltip_retains_long_message_paragraphs_and_follows_hover_not_selection()
-> Result<(), String> {
    let path = std::env::temp_dir().join(format!("gitcraft-graph-message-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
    let git = |args: &[&str]| git::run(Some(&path), args);
    git(&["init", "-q"])?;
    git(&["config", "user.name", "Graph test"])?;
    git(&["config", "user.email", "graph@example.invalid"])?;
    git(&["config", "commit.gpgSign", "false"])?;
    git(&["commit", "--allow-empty", "-qm", "Base"])?;
    let other = "Previous subject\n\nThe previous commit has its own full body.";
    git(&["commit", "--allow-empty", "-qm", other])?;
    let message = format!(
        "{}\n\n{}\n\nFinal paragraph: TAIL THAT MUST BE VISIBLE.",
        "Long Unicode café subject that will be elided in the graph ".repeat(3),
        "A detailed explanation with spacing and Unicode café. ".repeat(20)
    );
    let message_path = path.join("message.txt");
    std::fs::write(&message_path, &message).map_err(|e| e.to_string())?;
    git(&[
        "commit",
        "--allow-empty",
        "-q",
        "--cleanup=verbatim",
        "-F",
        &message_path.to_string_lossy(),
    ])?;
    // Use production Git and parse_record, rather than injecting a synthetic Commit.body.
    let commits = git::log(&path, None, Default::default(), 0, 10)?;
    assert_eq!(commits[0].body.chars().count(), 400);
    assert!(!commits[0].body.contains("TAIL THAT MUST BE VISIBLE"));
    let first = commits[0].hash.clone();
    let second = commits[1].hash.clone();
    let ctx = egui::Context::default();
    theme::install_fonts(&ctx);
    let mut tab = RepoTab::new_deferred(path.clone(), &ctx);
    tab.commits = commits.into_iter().map(Arc::new).collect();
    tab.graph = crate::graph::layout(&tab.commits);
    tab.rebuild_graph_labels();
    tab.selected = Some(2);
    let size = vec2(900.0, 220.0);
    frame(&ctx, &mut tab, size, vec![], 0.0);
    let (shapes, _) = frame(&ctx, &mut tab, size, vec![], 0.1);
    let first_point = painted_text_center(&shapes, &tab.commits[0].summary);
    let second_point = painted_text_center(&shapes, &tab.commits[1].summary);
    frame(
        &ctx,
        &mut tab,
        size,
        vec![Event::PointerMoved(first_point)],
        0.2,
    );
    frame(&ctx, &mut tab, size, vec![], 0.65); // Start the background request.
    frame(
        &ctx,
        &mut tab,
        size,
        vec![Event::PointerMoved(second_point)],
        0.7,
    );
    let wait_for = |tab: &mut RepoTab, hash: &str| -> Result<(), String> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while tab.graph_messages.message(&path, hash).is_none() {
            // Accept the real tooltip worker's result without starting unrelated readers.
            while let Ok(msg) = tab.rx.try_recv() {
                tab.handle(msg, &ctx);
            }
            if std::time::Instant::now() >= deadline {
                return Err("message worker did not finish".to_owned());
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        Ok(())
    };
    wait_for(&mut tab, &first)?;
    assert!(
        matches!(tab.graph_messages.message(&path, &first), Some(Ok(text)) if text == &message)
    );
    let (loading_second, _) = frame(&ctx, &mut tab, size, vec![], 1.5);
    assert!(
        !texts(&loading_second)
            .iter()
            .any(|s| s.galley.job.text == message),
        "an old hover result must never appear over the next row"
    );
    wait_for(&mut tab, &second)?;
    let (second_tooltip, _) = frame(&ctx, &mut tab, size, vec![], 2.0);
    assert!(
        texts(&second_tooltip)
            .iter()
            .any(|s| s.galley.job.text == other)
    );
    frame(
        &ctx,
        &mut tab,
        size,
        vec![Event::PointerMoved(first_point)],
        2.1,
    );
    frame(&ctx, &mut tab, size, vec![], 2.8);
    let (first_tooltip, _) = frame(&ctx, &mut tab, size, vec![], 3.2);
    assert!(
        texts(&first_tooltip)
            .iter()
            .any(|s| s.galley.job.text == message)
    );
    let message_geometry = |shapes: &[ClippedShape]| {
        shapes.iter().find_map(|clipped| match &clipped.shape {
            Shape::Text(text) if text.galley.job.text == message => {
                let tail = text
                    .galley
                    .rows
                    .last()?
                    .rect()
                    .translate(text.pos.to_vec2());
                Some((clipped.clip_rect, tail))
            }
            _ => None,
        })
    };
    let (clip, tail) = message_geometry(&first_tooltip).ok_or("missing message geometry")?;
    assert!(
        !clip.contains_rect(tail),
        "fixture must need scrolling to reach its tail"
    );
    let point = clip
        .intersect(Rect::from_min_size(Pos2::ZERO, size))
        .center();
    frame(&ctx, &mut tab, size, vec![Event::PointerMoved(point)], 3.21);
    frame(
        &ctx,
        &mut tab,
        size,
        vec![Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            phase: egui::TouchPhase::Move,
            delta: vec2(0.0, -4000.0),
            modifiers: egui::Modifiers::NONE,
        }],
        3.22,
    );
    let (scrolled_tooltip, _) = frame(&ctx, &mut tab, size, vec![], 3.25);
    let (clip, tail) =
        message_geometry(&scrolled_tooltip).ok_or("tooltip closed while scrolling")?;
    assert!(
        clip.contains_rect(tail),
        "final paragraph must be reachable: {tail:?} {clip:?}"
    );
    frame(
        &ctx,
        &mut tab,
        size,
        vec![Event::PointerMoved(first_point)],
        3.26,
    );
    assert_eq!(
        tab.selected,
        Some(2),
        "hover must not change the selected commit"
    );
    // Leave the scrollable popup before clicking its anchor: in this short viewport
    // the popup can cover the row. Re-enter and click before its hover delay elapses.
    frame(
        &ctx,
        &mut tab,
        size,
        vec![Event::PointerMoved(Pos2::new(1.0, 1.0))],
        3.3,
    );
    frame(
        &ctx,
        &mut tab,
        size,
        vec![Event::PointerMoved(first_point)],
        3.6,
    );
    frame(
        &ctx,
        &mut tab,
        size,
        vec![Event::PointerButton {
            pos: first_point,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }],
        3.61,
    );
    let (_, acts) = frame(
        &ctx,
        &mut tab,
        size,
        vec![Event::PointerButton {
            pos: first_point,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }],
        3.62,
    );
    assert!(acts.iter().any(|a| matches!(a, Action::Select(0))));
    drop(tab);
    assert!(git::commit_message(&path, "--output=written").is_err());
    std::fs::write(
        &message_path,
        "x".repeat(git::MAX_TOOLTIP_MESSAGE_BYTES + 1),
    )
    .map_err(|e| e.to_string())?;
    git(&[
        "commit",
        "--allow-empty",
        "-q",
        "--cleanup=verbatim",
        "-F",
        &message_path.to_string_lossy(),
    ])?;
    let oversized = git(&["rev-parse", "HEAD"])?;
    assert!(
        git::commit_message(&path, oversized.trim()).is_err_and(|error| error.contains("256 KiB")),
        "oversized objects must be refused before retaining full message text"
    );
    let _ = std::fs::remove_dir_all(&path);
    Ok(())
}
