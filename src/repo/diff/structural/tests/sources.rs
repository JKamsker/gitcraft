#[test]
fn sources_distinguish_commit_index_working_tree_root_and_deletion() -> Result<(), String> {
    let repo = Repo::new()?;
    let path = "code ü.rs";
    repo.write(path, "fn a() { f(1); }\n")?;
    let first = repo.commit()?;
    assert_eq!(
        reader::read_sides(
            &repo.0,
            &DiffKey::Commit {
                hash: first.clone(),
                path: path.into()
            }
        )?,
        (String::new(), "fn a() { f(1); }\n".into())
    );
    repo.write(path, "fn a() { f(2); }\n")?;
    repo.git(&["add", "--", path])?;
    repo.write(path, "fn a() { f(3); }\n")?;
    assert_eq!(
        reader::read_sides(&repo.0, &work(path, Area::Staged, 'M'))?,
        ("fn a() { f(1); }\n".into(), "fn a() { f(2); }\n".into())
    );
    assert_eq!(
        reader::read_sides(&repo.0, &work(path, Area::Unstaged, 'M'))?,
        ("fn a() { f(2); }\n".into(), "fn a() { f(3); }\n".into())
    );
    assert_eq!(
        reader::read_sides(
            &repo.0,
            &DiffKey::Range {
                from: first,
                to: None,
                path: path.into()
            }
        )?,
        ("fn a() { f(1); }\n".into(), "fn a() { f(3); }\n".into())
    );
    repo.git(&["rm", "-f", "--", path])?;
    assert_eq!(
        reader::read_sides(&repo.0, &work(path, Area::Staged, 'D'))?,
        ("fn a() { f(1); }\n".into(), String::new())
    );
    let deleted = repo.commit()?;
    assert_eq!(
        reader::read_sides(
            &repo.0,
            &DiffKey::Commit {
                hash: deleted,
                path: path.into()
            }
        )?,
        ("fn a() { f(1); }\n".into(), String::new())
    );
    Ok(())
}

#[test]
fn rename_uses_the_old_name_even_when_git_quotes_it() -> Result<(), String> {
    let repo = Repo::new()?;
    // Git quotes UTF-8 names with core.quotePath=true on every platform. Double
    // quotes in filenames are additionally covered on Unix, where they are legal.
    let old = if cfg!(windows) {
        "old ü.rs"
    } else {
        "old \"ü.rs"
    };
    let new = "new ü.rs";
    repo.git(&["config", "core.quotePath", "true"])?;
    repo.write(old, "fn a() {}\n")?;
    let before = repo.commit()?;
    repo.git(&["mv", "--", old, new])?;
    let quoted = repo.git(&["diff", "--cached", "--name-status", "-M"])?;
    assert!(
        quoted.contains("\\303\\274"),
        "Git did not quote the Unicode path: {quoted}"
    );
    let key = DiffKey::Work(StatusFile {
        path: new.into(),
        old_path: Some(old.into()),
        code: 'R',
        area: Area::Staged,
    });
    assert_eq!(
        reader::read_sides(&repo.0, &key)?,
        ("fn a() {}\n".into(), "fn a() {}\n".into())
    );
    let after = repo.commit()?;
    for key in [
        DiffKey::Commit {
            hash: after.clone(),
            path: new.into(),
        },
        DiffKey::Range {
            from: before,
            to: Some(after),
            path: new.into(),
        },
    ] {
        assert_eq!(
            reader::read_sides(&repo.0, &key)?,
            ("fn a() {}\n".into(), "fn a() {}\n".into())
        );
    }
    Ok(())
}

#[test]
fn unreadable_binary_non_utf8_large_and_conflicted_sources_fall_back() -> Result<(), String> {
    let repo = Repo::new()?;
    repo.write("file.rs", "fn a() {}\n")?;
    repo.commit()?;
    std::fs::remove_file(repo.0.join("file.rs")).map_err(|e| e.to_string())?;
    assert!(reader::read_sides(&repo.0, &work("file.rs", Area::Unstaged, 'M')).is_err());
    for data in [vec![0], vec![0xff], vec![b'x'; reader::MAX_BYTES + 1]] {
        repo.write("file.rs", data)?;
        assert!(reader::read_sides(&repo.0, &work("file.rs", Area::Unstaged, 'M')).is_err());
    }
    assert!(reader::read_sides(&repo.0, &work("file.rs", Area::Conflicted, 'U')).is_err());
    // Size limits also apply to committed blobs, before reading their contents.
    repo.commit()?;
    assert!(reader::read_sides(&repo.0, &work("file.rs", Area::Staged, 'M')).is_err());
    Ok(())
}

#[test]
fn report_keeps_only_novel_tokens_and_checks_utf8_byte_ranges() -> Result<(), String> {
    let old = "fn f() { call(\"ü\", 1); }\n";
    let new = "fn f() {\n  call(\"ü\",\n       2);\n}\n";
    let report = br#"{"language":"Rust","status":"changed","chunks":[[
        {"lhs":{"line_number":0,"changes":[{"start":20,"end":21,"content":"1"}]},"rhs":{"line_number":0,"changes":[]}},
        {"rhs":{"line_number":2,"changes":[{"start":7,"end":8,"content":"2"}]}}
    ]]}"#;
    let data = model::parse(report, old, new)?;
    assert_eq!(data.language, "Rust");
    assert!(!data.unchanged);
    let model::Row::Pair(Some(before), Some(after)) = &data.rows[1] else {
        return Err("no paired row".into());
    };
    assert_eq!(&before.text[before.changed[0].clone()], "1");
    assert!(after.changed.is_empty());
    let model::Row::Pair(None, Some(after)) = &data.rows[2] else {
        return Err("no wrapped row".into());
    };
    assert_eq!(&after.text[after.changed[0].clone()], "2");
    let bad = String::from_utf8_lossy(report).replace(
        "\"start\":20,\"end\":21,\"content\":\"1\"",
        "\"start\":16,\"end\":17,\"content\":\"ü\"",
    );
    assert!(model::parse(bad.as_bytes(), old, new).is_err());
    assert!(model::parse(b"{}", old, new).is_err());
    assert!(model::parse(br#"{"language":"Rust","status":"changed"}"#, old, new).is_err());
    Ok(())
}

#[test]
fn unsupported_languages_parse_errors_and_resource_fallbacks_are_explained() {
    for language in [
        "Text",
        "Text (1 Rust parse error, exceeded DFT_PARSE_ERROR_LIMIT)",
        "Text (exceeded DFT_GRAPH_LIMIT)",
        "Binary",
    ] {
        let report = serde_json::json!({"language":language,"status":"unchanged"});
        assert!(model::parse(report.to_string().as_bytes(), "a", "a").is_err());
    }
    for (status, a, b) in [
        ("created", "", "fn a() {}\n"),
        ("deleted", "fn a() {}\n", ""),
    ] {
        let report = serde_json::json!({"language":"Rust","status":status});
        let result = model::parse(report.to_string().as_bytes(), a, b);
        assert!(result.is_ok());
    }
}

#[test]
fn stale_answers_cannot_replace_a_newer_diff_or_read() -> Result<(), String> {
    let ctx = egui::Context::default();
    let repo = Repo::new()?;
    let mut tab = RepoTab::new_deferred(repo.0.clone(), &ctx);
    let mut view = DiffView::new(work("a.rs", Area::Untracked, '?'));
    let current = view.next_request();
    view.structural = State {
        enabled: true,
        load: 2,
        result: None,
        running: Some((current, 2)),
        reload: false,
    };
    tab.diff = Some(view);
    tab.structural_running = Some((current, 2));
    for ids in [(current - 1, 2), (current, 1)] {
        tab.tx
            .send(Msg::DiffStructural(ids, Box::new(Err("stale".into()))))
            .map_err(|e| e.to_string())?;
        tab.poll(&ctx);
        assert!(
            tab.diff
                .as_ref()
                .is_some_and(|v| v.structural.result.is_none())
        );
    }
    tab.tx
        .send(Msg::DiffStructural(
            (current, 2),
            Box::new(Err("current".into())),
        ))
        .map_err(|e| e.to_string())?;
    tab.poll(&ctx);
    assert!(
        tab.diff
            .as_ref()
            .is_some_and(|v| matches!(&v.structural.result,Some(Err(e)) if e == "current"))
    );
    tab.apply(Action::DiffStructural(false), &ctx);
    assert!(tab.diff.as_ref().is_some_and(|v| !v.structural.enabled));
    Ok(())
}
