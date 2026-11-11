/// A real Difftastic run, when installed; parser/reader/transport contract tests always run.
#[test]
fn real_difftastic_ignores_rewrapping_but_keeps_changed_expressions() -> Result<(), String> {
    if Command::new("difft").arg("--version").output().is_err() {
        eprintln!("difft not installed; skipping the live-tool comparison");
        return Ok(());
    }
    let repo = Repo::new()?;
    repo.write("code ü.rs", "fn f() { call(1, 2); }\n")?;
    repo.commit()?;
    repo.write("code ü.rs", "fn f() {\n  call(1,\n       2);\n}\n")?;
    let key = work("code ü.rs", Area::Unstaged, 'M');
    assert!(reader::load(&repo.0, &key)?.unchanged);
    repo.write("code ü.rs", "fn f() {\n  call(1,\n       3);\n}\n")?;
    let result = reader::load(&repo.0, &key)?;
    let changed: Vec<_> = result
        .rows
        .iter()
        .filter_map(|r| match r {
            model::Row::Pair(a, b) => Some([a.as_ref(), b.as_ref()]),
            _ => None,
        })
        .flatten()
        .flatten()
        .flat_map(|l| l.changed.iter().map(|r| &l.text[r.clone()]))
        .collect();
    assert_eq!(changed, ["2", "3"]);
    repo.write("code ü.rs", "fn f( {\n")?;
    assert!(
        reader::load(&repo.0, &key)
            .err()
            .is_some_and(|e| e.contains("parse error"))
    );
    repo.write("file.unsupported", "arbitrary text\n")?;
    assert!(reader::load(&repo.0, &work("file.unsupported", Area::Untracked, '?')).is_err());
    Ok(())
}

#[cfg(unix)]
#[test]
fn remote_request_is_data_safe_and_runs_the_same_real_tool() -> Result<(), String> {
    if Command::new("difft").arg("--version").output().is_err() {
        return Ok(());
    }
    let repo = Repo::new()?;
    let old = "fn f() { call(1); }\n";
    let new = "fn f() { call(2); }\n";
    let marker = crate::remote::sh_quote(repo.0.join("gotcha").to_str().ok_or("marker path")?);
    let report = remote_compare(&repo, &format!("x $(touch {marker}) ü.rs"), old, new)?;
    assert!(model::parse(&report, old, new).is_ok());
    assert!(!repo.0.join("gotcha").exists());
    Ok(())
}

#[cfg(unix)]
fn remote_compare(repo: &Repo, path: &str, old: &str, new: &str) -> Result<Vec<u8>, String> {
    use std::io::Write;
    use std::process::Stdio;
    let target = crate::remote::Target {
        host: crate::remote::Host::Ssh("test".into()),
        path: repo.0.display().to_string(),
    };
    let request = tool::request(&target, path, old, new);
    assert!(!request.script.contains(old));
    let mut command = Command::new("sh");
    command
        .current_dir(&repo.0)
        .arg("-c")
        .arg(&request.script)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for name in request.unset {
        command.env_remove(name);
    }
    for (name, value) in request.env {
        command.env(name, value);
    }
    let mut child = command.spawn().map_err(|e| e.to_string())?;
    child
        .stdin
        .take()
        .ok_or("no stdin")?
        .write_all(&request.stdin.unwrap_or_default())
        .map_err(|e| e.to_string())?;
    let output = child.wait_with_output().map_err(|e| e.to_string())?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(output.stdout)
}

#[cfg(unix)]
#[test]
fn a_hung_tool_or_inherited_pipe_times_out_and_large_output_is_bounded() -> Result<(), String> {
    let start = Instant::now();
    let mut hung = Command::new("sh");
    hung.args(["-c", "sleep 30 & wait"]);
    assert!(tool::run(hung, Duration::from_millis(100)).is_err());
    assert!(start.elapsed() < Duration::from_secs(3));
    let mut pipe = Command::new("sh");
    pipe.args(["-c", "sleep 30 & exit 0"]);
    assert!(tool::run(pipe, Duration::from_millis(100)).is_err());
    let mut huge = Command::new("sh");
    huge.args(["-c", "head -c 10000000 /dev/zero"]);
    assert!(tool::run(huge, Duration::from_secs(3)).is_err());
    Ok(())
}

#[test]
fn syntax_view_draws_read_only_and_patch_fallback_cannot_stage_lines() -> Result<(), String> {
    let repo = Repo::new()?;
    repo.write("a.rs", "fn f() {}\n")?;
    repo.commit()?;
    repo.write("a.rs", "fn f() { x(); }\n")?;
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(repo.0.clone(), &ctx);
    let mut view = DiffView::new(work("a.rs", Area::Unstaged, 'M'));
    view.data = Some(crate::repo::diff::load(
        &repo.0,
        &view.key,
        git::DiffOptions::default(),
        true,
    ));
    view.structural.enabled = true;
    view.structural.result = Some(Err("unsupported language".into()));
    view.chosen.insert(1);
    tab.diff = Some(view);
    let mut actions = Vec::new();
    crate::theme::install_fonts(&ctx);
    let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
        egui::CentralPanel::default().show(ui, |ui| tab.diff_view(ui, &mut actions));
    });
    let text = out
        .shapes
        .iter()
        .filter_map(|s| match &s.shape {
            egui::Shape::Text(t) => Some(t.galley.text()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    out.textures_delta.clear();
    assert!(text.contains("Showing the patch"), "{text}");
    assert!(!text.contains("Stage file"), "{text}");
    assert!(!text.contains("Stage hunk"), "{text}");
    assert!(!text.contains("Stage 1 line"), "{text}");
    assert!(tab.find_in().is_some(), "patch fallback retains Find");
    tab.apply(Action::DiffStep(-1), &ctx);
    assert!(
        tab.diff.as_ref().is_some_and(|v| v.jump.is_some()),
        "patch fallback retains change navigation"
    );
    if let Some(view) = &mut tab.diff {
        view.structural.result = Some(model::parse(
            br#"{"language":"Rust","status":"created"}"#,
            "",
            "fn f() {}",
        ));
        view.jump = None;
        view.key = DiffKey::Commit {
            hash: git::head(&repo.0),
            path: "a.rs".to_owned(),
        };
    }
    assert!(
        tab.find_in().is_some(),
        "successful Syntax searches its rendered text"
    );
    tab.apply(Action::DiffStep(-1), &ctx);
    assert!(tab.diff.as_ref().is_some_and(|v| v.jump.is_none()));
    let mut syntax = ctx.run_ui(egui::RawInput::default(), |ui| {
        egui::CentralPanel::default().show(ui, |ui| tab.diff_view(ui, &mut actions));
    });
    let labels = syntax
        .shapes
        .iter()
        .filter_map(|s| match &s.shape {
            egui::Shape::Text(t) => Some(t.galley.text()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    syntax.textures_delta.clear();
    assert!(
        !labels.contains("Revert file") && !labels.contains("Revert 1 line"),
        "{labels}"
    );
    tab.apply(Action::DiffStructural(false), &ctx);
    assert!(tab.diff.as_ref().is_some_and(|v| v.chosen.is_empty()));
    Ok(())
}
