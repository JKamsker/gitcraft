#[test]
fn rapid_refreshes_coalesce_and_a_closed_diff_cannot_accept_the_previous_job() {
    let mut state = State {
        enabled: true,
        load: 1,
        running: Some((10, 1)),
        reload: true,
        result: None,
    };
    assert!(state.arrived((10, 1), 12, Err("old".into())));
    assert!(state.result.is_none());
    assert!(state.running.is_none());
    let mut replacement = State {
        enabled: true,
        load: 1,
        running: Some((13, 1)),
        ..Default::default()
    };
    assert!(!replacement.arrived((10, 1), 13, Err("previous diff".into())));
    assert!(replacement.result.is_none());
    assert_eq!(replacement.running, Some((13, 1)));
}

#[test]
fn syntax_view_renders_unicode_source_and_no_syntax_changes() -> Result<(), String> {
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    for (status, old, new, expected) in [
        ("created", "", "fn ü() {\n\tcall(2);\n}\n", "call(2)"),
        ("unchanged", "fn a() {}", "fn a() { }", "No syntax changes"),
    ] {
        let report = serde_json::json!({"language":"Rust","status":status});
        let data = model::parse(report.to_string().as_bytes(), old, new)?;
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| super::view::show(ui, &data));
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
        assert!(text.contains(expected), "{text}");
    }
    Ok(())
}

#[test]
fn temporary_writes_preserve_outside_files_for_absolute_and_windows_tree_names()
-> Result<(), String> {
    let repo = Repo::new()?;
    let outside = repo.0.join("victim.rs");
    std::fs::write(&outside, b"keep this outside file").map_err(|e| e.to_string())?;
    let mut paths = vec![
        outside.display().to_string(),
        r"C:\victim.rs".to_owned(),
        r"C:victim.rs".to_owned(),
        r"\victim.rs".to_owned(),
        r"..\victim.rs".to_owned(),
        r"\\server\share\victim.rs".to_owned(),
        r"\\?\C:\victim.rs".to_owned(),
        r"\\.\C:\victim.rs".to_owned(),
        "../victim.rs".to_owned(),
        "code ü.rs".to_owned(),
        "--override=*.rs:Text.rs".to_owned(),
        "nested/..\\..\\victim.rs".to_owned(),
        format!(
            "$(touch {}).rs",
            crate::remote::sh_quote(repo.0.join("gotcha").to_str().ok_or("marker path")?)
        ),
        format!(
            "x $(touch {}) ü.rs",
            crate::remote::sh_quote(repo.0.join("gotcha").to_str().ok_or("marker path")?)
        ),
    ];
    // On native Windows these reproduce drive-absolute, drive-relative, rooted and
    // backslash traversal forms targeting an existing file, with no drive assumptions.
    #[cfg(windows)]
    {
        use std::path::Component;
        let drive = outside
            .components()
            .find_map(|c| match c {
                Component::Prefix(p) => Some(p.as_os_str().to_string_lossy().into_owned()),
                _ => None,
            })
            .ok_or("no Windows drive prefix")?;
        paths.push(format!("{drive}victim.rs"));
        paths.push(format!(r"{drive}\victim.rs"));
        paths.push(format!(
            r"..\..\{}",
            outside.file_name().unwrap_or_default().to_string_lossy()
        ));
    }
    for path in paths.drain(..) {
        let (_temp, before, after) = tool::prepare("fn old() {}", "fn new() {}")?;
        let root = before
            .parent()
            .and_then(std::path::Path::parent)
            .ok_or("no temp root")?;
        assert_eq!(before, root.join("before").join("source"), "{path:?}");
        assert_eq!(after, root.join("after").join("source"), "{path:?}");
        assert_ne!(before, outside);
        assert_ne!(after, outside);
        let args = tool::protocol_args(&path, &before, &after);
        assert_eq!(args[0], std::ffi::OsStr::new(&path));
        assert_eq!(args[1], before.as_os_str());
        assert_eq!(args[4], after.as_os_str());
        assert_eq!(args[2], "0000000");
        assert_eq!(args[3], "100644");
        assert_eq!(args[5], "0000000");
        assert_eq!(args[6], "100644");
        if Command::new("difft").arg("--version").output().is_ok() {
            let report = tool::compare(&repo.0, &path, "fn old() {}", "fn new() {}")?;
            assert!(
                model::parse(&report, "fn old() {}", "fn new() {}").is_ok(),
                "{path:?}"
            );
            #[cfg(unix)]
            {
                let report = remote_compare(&repo, &path, "fn old() {}", "fn new() {}")?;
                assert!(
                    model::parse(&report, "fn old() {}", "fn new() {}").is_ok(),
                    "{path:?}"
                );
            }
        }
        assert_eq!(
            std::fs::read(&before).map_err(|e| e.to_string())?,
            b"fn old() {}"
        );
        assert_eq!(
            std::fs::read(&after).map_err(|e| e.to_string())?,
            b"fn new() {}"
        );
        assert_eq!(
            std::fs::read(&outside).map_err(|e| e.to_string())?,
            b"keep this outside file"
        );
    }
    assert!(!repo.0.join("gotcha").exists());
    Ok(())
}

#[test]
fn safe_staging_preserves_real_difftastic_language_detection() -> Result<(), String> {
    if Command::new("difft").arg("--version").output().is_err() {
        return Ok(());
    }
    let repo = Repo::new()?;
    for (path, old, new, language) in [
        (
            r"C:\victim.rs",
            "fn f() { x(1); }",
            "fn f() { x(2); }",
            "Rust",
        ),
        ("code ü.rs", "fn f() { x(1); }", "fn f() { x(2); }", "Rust"),
        ("日本語.py", "x = 1\n", "x = 2\n", "Python"),
        (
            "Dockerfile",
            "FROM alpine:3.20\n",
            "FROM alpine:3.21\n",
            "Dockerfile",
        ),
        ("CMakeLists.txt", "set(X 1)\n", "set(X 2)\n", "CMake"),
        ("Cargo.lock", "version = 3\n", "version = 4\n", "TOML"),
        ("composer.lock", "{\"a\":1}\n", "{\"a\":2}\n", "JSON"),
        ("Pipfile.lock", "{\"a\":1}\n", "{\"a\":2}\n", "JSON"),
        (
            "yarn.lock",
            "foo:\n  version: \"1\"\n",
            "foo:\n  version: \"2\"\n",
            "YAML",
        ),
        (
            ".emacs",
            "(message \"one\")\n",
            "(message \"two\")\n",
            "Emacs Lisp",
        ),
        (
            "rebar.config",
            "{one, true}.\n",
            "{one, false}.\n",
            "Erlang",
        ),
        (
            "App.config",
            "<root>one</root>\n",
            "<root>two</root>\n",
            "XML",
        ),
        ("TARGETS", "x = 1\n", "x = 2\n", "Python"),
        ("CITATION.cff", "title: one\n", "title: two\n", "YAML"),
        (
            "file.app.src",
            "{application, one, []}.\n",
            "{application, two, []}.\n",
            "Erlang",
        ),
        ("state.tfstate.backup", "{\"a\":1}\n", "{\"a\":2}\n", "JSON"),
    ] {
        let report = tool::compare(&repo.0, path, old, new)?;
        let parsed = model::parse(&report, old, new)?;
        assert_eq!(parsed.language, language, "{path}");
        assert!(!parsed.unchanged);
        #[cfg(unix)]
        {
            let report = remote_compare(&repo, path, old, new)?;
            let parsed = model::parse(&report, old, new)?;
            assert_eq!(parsed.language, language, "remote {path}");
            assert!(!parsed.unchanged);
        }
    }
    assert!(!repo.0.join("gotcha").exists());
    Ok(())
}

/// Exercise native Windows absolute Git names as display metadata, including
/// names that Windows cannot check out. This also runs with an absolute UNC temp folder.
#[cfg(windows)]
#[test]
fn windows_absolute_git_name_cannot_overwrite_an_existing_file() -> Result<(), String> {
    let repo = Repo::new()?;
    let outside = repo.0.join("outside.rs");
    std::fs::write(&outside, b"preserve native Windows file").map_err(|e| e.to_string())?;
    let git_name = outside
        .to_str()
        .ok_or("non-UTF8 test path")?
        .replace('/', "\\");
    assert!(std::path::Path::new(&git_name).is_absolute());
    if Command::new("difft").arg("--version").output().is_ok() {
        let report = tool::compare(&repo.0, &git_name, "fn old() {}", "fn new() {}")?;
        assert!(model::parse(&report, "fn old() {}", "fn new() {}").is_ok());
    }
    let (_temp, before, after) = tool::prepare("old source", "new source")?;
    assert_eq!(before.file_name().and_then(|n| n.to_str()), Some("source"));
    assert_eq!(after.file_name().and_then(|n| n.to_str()), Some("source"));
    assert_ne!(before, outside);
    assert_ne!(after, outside);
    assert_eq!(
        std::fs::read(&outside).map_err(|e| e.to_string())?,
        b"preserve native Windows file"
    );
    assert_eq!(
        std::fs::read(&before).map_err(|e| e.to_string())?,
        b"old source"
    );
    assert_eq!(
        std::fs::read(&after).map_err(|e| e.to_string())?,
        b"new source"
    );
    Ok(())
}
