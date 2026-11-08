use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(unix)]
use std::time::{Duration, Instant};

use super::{State, model, reader, tool};
use crate::git::{self, Area, StatusFile};
use crate::repo::{Action, DiffKey, DiffView, Msg, RepoTab};

struct Repo(PathBuf);
impl Repo {
    fn new() -> Result<Self, String> {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "gitcraft-structural ü {}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let repo = Self(dir);
        for args in [
            vec!["init", "-q"],
            vec!["config", "user.name", "Test"],
            vec!["config", "user.email", "test@example.org"],
            vec!["config", "commit.gpgSign", "false"],
        ] {
            repo.git(&args)?;
        }
        Ok(repo)
    }
    fn git(&self, args: &[&str]) -> Result<String, String> {
        git::run(Some(&self.0), args)
    }
    fn write(&self, path: &str, contents: impl AsRef<[u8]>) -> Result<(), String> {
        std::fs::write(self.0.join(path), contents).map_err(|e| e.to_string())
    }
    fn commit(&self) -> Result<String, String> {
        self.git(&["add", "-A"])?;
        self.git(&["commit", "-qm", "change"])?;
        Ok(git::head(&self.0))
    }
}
impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn work(path: &str, area: Area, code: char) -> DiffKey {
    DiffKey::Work(StatusFile {
        path: path.to_owned(),
        old_path: None,
        code,
        area,
    })
}

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
