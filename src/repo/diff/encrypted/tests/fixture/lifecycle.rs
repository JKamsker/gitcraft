#[test]
fn decrypt_action_is_asynchronous_and_close_cancels_its_private_worker() -> Result<(), String> {
    let repo = Repo::new()?;
    let path = "secret.json";
    repo.write(path, &json("age1fixture", "cipher1"))?;
    repo.commit()?;
    repo.write(path, &json("age1fixture", "cipher2"))?;
    let started = repo.0.join("started");
    let late = repo.0.join("late");
    program(
        &repo,
        &format!(
            "cat >/dev/null; printf started > {}; sleep 0.6; printf 'fixture-private-output'; printf late > {}",
            crate::remote::sh_quote(&started.to_string_lossy()),
            crate::remote::sh_quote(&late.to_string_lossy())
        ),
    )?;
    let ctx = egui::Context::default();
    let mut tab = crate::repo::RepoTab::new_deferred(repo.0.clone(), &ctx);
    let mut view = crate::repo::DiffView::new(work(path, Area::Unstaged, 'M'));
    let request = view.next_request();
    view.arrived(
        request,
        crate::repo::diff::load(&repo.0, &view.key, git::DiffOptions::default(), true),
    );
    tab.diff = Some(view);
    let start = std::time::Instant::now();
    tab.apply(crate::repo::Action::DiffDecrypted(true), &ctx);
    assert!(start.elapsed() < Duration::from_millis(200));
    let active = tab
        .diff
        .as_ref()
        .and_then(|v| v.encrypted.active.clone())
        .ok_or("Private job not started")?;
    let until = std::time::Instant::now() + Duration::from_secs(2);
    while !started.exists() && std::time::Instant::now() < until {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(started.exists());
    tab.apply(crate::repo::Action::CloseDiff, &ctx);
    assert!(!active.load(Ordering::Acquire));
    std::thread::sleep(Duration::from_millis(700));
    assert!(!late.exists());
    assert!(tab.diff.is_none());
    assert!(tab.status.is_none() && tab.finished.is_empty());
    Ok(())
}

#[test]
fn private_mode_and_stale_generation_refuse_queued_raw_hunk_writes() -> Result<(), String> {
    let repo = Repo::new()?;
    let path = "secret.json";
    repo.write(path, &json("age1fixture", "cipher1"))?;
    repo.commit()?;
    repo.write(path, &json("age1fixture", "cipher2"))?;
    let ctx = egui::Context::default();
    let mut tab = crate::repo::RepoTab::new_deferred(repo.0.clone(), &ctx);
    let mut view = crate::repo::DiffView::new(work(path, Area::Unstaged, 'M'));
    let request = view.next_request();
    view.arrived(
        request,
        crate::repo::diff::load(&repo.0, &view.key, git::DiffOptions::default(), true),
    );
    let data = view
        .data
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .ok_or("No fixture patch")?;
    let hunk = data
        .lines
        .iter()
        .position(|l| l.kind == crate::repo::diff::Kind::Hunk)
        .ok_or("No fixture hunk")?;
    let lines = crate::repo::diff::hunk_changes(&data.lines, hunk);
    view.encrypted.begin();
    tab.diff = Some(view);
    tab.apply(
        crate::repo::Action::PickLines {
            pick: crate::repo::diff::Pick::Stage,
            lines: lines.clone(),
        },
        &ctx,
    );
    if let Some(view) = &mut tab.diff {
        view.encrypted.close();
        view.next_request();
    }
    tab.apply(
        crate::repo::Action::PickLines {
            pick: crate::repo::diff::Pick::Stage,
            lines: lines.clone(),
        },
        &ctx,
    );
    assert_eq!(
        repo.git(&["show", &format!(":{path}")])?.trim(),
        json("age1fixture", "cipher1")
    );
    Ok(())
}

#[test]
fn sops_command_is_local_and_has_only_fixed_format_arguments() {
    // Pure command construction. There is no repository/host parameter to route it.
    let command = tool::command(Format::Json);
    assert_eq!(command.get_program(), std::ffi::OsStr::new("sops"));
    assert_eq!(
        command.get_current_dir(),
        Some(std::env::temp_dir().as_path())
    );
    assert_eq!(
        command.get_args().collect::<Vec<_>>(),
        tool::args(Format::Json).map(std::ffi::OsStr::new)
    );
}
