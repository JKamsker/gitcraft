#[test]
fn loader_coalesces_replacements_across_views_and_ignores_old_completions() -> Result<(), String> {
    let repo = Repo::new()?;
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(repo.0.clone(), &ctx);
    let mut old = DiffView::new(work("old.rs", Area::Untracked, '?'));
    let old_ids = (old.next_request(), 1);
    old.structural.enabled = true;
    old.structural.running = Some(old_ids);
    old.structural.load = 1;
    tab.structural_running = Some(old_ids);
    tab.diff = Some(old);
    for _ in 0..5 {
        tab.load_diff_structural(&ctx);
    }
    let mut latest = DiffView::new(work("missing-latest.rs", Area::Untracked, '?'));
    latest.next_request();
    latest.structural.enabled = true;
    tab.diff = Some(latest);
    for _ in 0..5 {
        tab.load_diff_structural(&ctx);
    }
    assert_eq!(tab.structural_running, Some(old_ids));
    assert_eq!(tab.diff.as_ref().map(|v| v.structural.load), Some(0));
    tab.tx
        .send(Msg::DiffStructural(
            old_ids,
            Box::new(Err("obsolete".into())),
        ))
        .map_err(|e| e.to_string())?;
    tab.poll(&ctx);
    assert_eq!(tab.diff.as_ref().map(|v| v.structural.load), Some(1));
    assert!(!tab.structural_pending);
    tab.tx
        .send(Msg::DiffStructural(
            old_ids,
            Box::new(Err("obsolete again".into())),
        ))
        .map_err(|e| e.to_string())?;
    tab.poll(&ctx);
    assert_eq!(tab.diff.as_ref().map(|v| v.structural.load), Some(1));
    assert!(tab.diff.as_ref().is_some_and(|v| {
        v.structural
            .result
            .as_ref()
            .is_none_or(|r| !matches!(r, Err(e) if e.contains("obsolete")))
    }));
    Ok(())
}

#[test]
fn unstaged_rename_reads_the_original_index_path() -> Result<(), String> {
    let repo = Repo::new()?;
    repo.write("old.rs", "fn old() {}\n")?;
    repo.commit()?;
    std::fs::rename(repo.0.join("old.rs"), repo.0.join("new.rs")).map_err(|e| e.to_string())?;
    repo.git(&["add", "-N", "new.rs"])?;
    let file = git::status(&repo.0)?
        .unstaged
        .into_iter()
        .find(|f| f.path == "new.rs")
        .ok_or("renamed status")?;
    assert_eq!(file.code, 'R');
    assert_eq!(file.old_path.as_deref(), Some("old.rs"));
    assert_eq!(
        reader::read_sides(&repo.0, &DiffKey::Work(file))?,
        ("fn old() {}\n".to_owned(), "fn old() {}\n".to_owned())
    );
    Ok(())
}

#[test]
fn remote_preparation_falls_back_before_starting_a_tool_or_transport() {
    let key = work("file.rs", Area::Untracked, '?');
    let result = reader::read_sides(std::path::Path::new("ssh://example.invalid/repo"), &key);
    assert!(matches!(result, Err(e) if e.contains("local repositories")));
}

/// No process is launched: this records how both runners pass untrusted display metadata.
#[test]
fn syntax_supervision_protocol_is_data_only() {
    let before = PathBuf::from("private-before");
    let after = PathBuf::from("private-after");
    let display = "../../user; touch unexpected";
    let args = tool::protocol_args(display, &before, &after);
    assert_eq!(args[0], display);
    assert_eq!(args[1], before.as_os_str());
    assert_eq!(args[4], after.as_os_str());
    assert_eq!(args.len(), 7);
}

#[cfg(unix)]
#[test]
fn closing_tab_cancels_preparation_and_tool_and_prevents_later_effects() -> Result<(), String> {
    for preparation in [true, false] {
        let repo = Repo::new()?;
        let ready = repo.0.join("ready");
        let effect = repo.0.join("survived");
        let script = format!(
            "printf ready > {}; sleep 1; printf survived > {}",
            crate::remote::sh_quote(&ready.to_string_lossy()),
            crate::remote::sh_quote(&effect.to_string_lossy())
        );
        let ctx = egui::Context::default();
        let tab = RepoTab::new_deferred(repo.0.clone(), &ctx);
        let path = repo.0.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        tab.then_or(
            &ctx,
            false,
            Err,
            move || {
                let result = if preparation {
                    git::syntax_read(
                        &path,
                        &["-c", &format!("alias.block=!{script}"), "block"],
                        4096,
                        Duration::from_secs(30),
                    )
                    .map(|_| ())
                } else {
                    let mut command = Command::new("sh");
                    command.args(["-c", &script]);
                    tool::run(command, Duration::from_secs(30)).map(|_| ())
                };
                let _ = tx.send(result.clone());
                result
            },
            |_, _, _| {},
        );
        let start = Instant::now();
        while !ready.exists() && start.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(ready.exists(), "owned child did not start");
        drop(tab);
        let result = rx
            .recv_timeout(Duration::from_secs(5))
            .map_err(|e| e.to_string())?;
        assert!(result.is_err_and(|e| e.contains("cancelled")));
        std::thread::sleep(Duration::from_millis(1100));
        assert!(!effect.exists(), "child survived tab cancellation");
    }
    Ok(())
}
