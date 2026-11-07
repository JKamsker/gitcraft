use super::model::{Format, inspect};
use super::*;

fn json(recipient: &str, content: &str) -> String {
    serde_json::json!({"secret":format!("ENC[{content}]"),"sops":{"age":[{"recipient":recipient,"enc":"wrapped"}],"mac":"ENC[mac]","version":"3.9.0"}}).to_string()
}
fn plaintext_compare(old: &str, new: &str, a: &str, b: &str) -> Result<Comparison, String> {
    let mut n = 0;
    compare("secret.json", old, new, &|| true, |_, _| {
        n += 1;
        Ok(if n == 1 { a.into() } else { b.into() })
    })
}
#[test]
fn content_changes_are_distinct_from_recipients_and_reencryption() -> Result<(), String> {
    let old = json("age1old", "cipher1");
    let recipient = json("age1new", "cipher1");
    let view = plaintext_compare(
        &old,
        &recipient,
        "password: fixture\n",
        "password: fixture\n",
    )?;
    assert!(!view.content_changed && view.recipients_changed && view.metadata_changed);
    let reencryption = json("age1old", "cipher2");
    let view = plaintext_compare(
        &old,
        &reencryption,
        "password: fixture\n",
        "password: fixture\n",
    )?;
    assert!(!view.content_changed && !view.recipients_changed && !view.metadata_changed);
    let view = plaintext_compare(&old, &reencryption, "password: old\n", "password: new\n")?;
    assert!(view.content_changed && !view.recipients_changed);
    assert!(
        view.rows
            .iter()
            .any(|row| row.kind == similar::ChangeTag::Delete && row.text == "password: old")
    );
    assert!(
        view.rows
            .iter()
            .any(|row| row.kind == similar::ChangeTag::Insert && row.text == "password: new")
    );
    Ok(())
}
#[test]
fn formats_recognize_sops_metadata_without_invoking_a_tool() -> Result<(), String> {
    let examples=[
        ("secret.json",json("age1fixture","cipher"),Format::Json),
        ("secret.yaml","secret: ENC[cipher]\nsops:\n  age:\n    - recipient: age1fixture\n      enc: wrapped\n  mac: ENC[mac]\n  version: 3.9.0\n".into(),Format::Yaml),
        (".env","SECRET=ENC[cipher]\nsops_age__list_0__map_recipient=age1fixture\nsops_mac=ENC[mac]\nsops_version=3.9.0\n".into(),Format::Dotenv),
        ("secret.ini","[app]\nsecret=ENC[cipher]\n[sops]\nage__list_0__map_recipient=age1fixture\nmac=ENC[mac]\nversion=3.9.0\n".into(),Format::Ini),
    ];
    for (path, text, format) in examples {
        assert_eq!(inspect(path, &text)?.map(|e| e.format), Some(format));
    }
    assert_eq!(
        inspect(
            "secret.yaml",
            "{secret: 'ENC[cipher]', sops: {version: '3.10.0', mac: 'ENC[mac]'}}"
        )?
        .map(|e| e.format),
        Some(Format::Yaml)
    );
    assert_eq!(
        inspect(
            "opaque.bin",
            r#"{"data":"ENC[cipher]","sops":{"version":"3.10.0","mac":"ENC[mac]"}}"#
        )?
        .map(|e| e.format),
        Some(Format::Binary)
    );
    assert!(inspect("ordinary.json", r#"{"secret":"ENC[looks similar]"}"#)?.is_none());
    assert!(inspect("ordinary.yaml", "normal: value\n")?.is_none());
    assert!(
        inspect(
            "ordinary.json",
            r#"{"nested":{"sops":{"version":"3.9.0","mac":"ENC[mac]"}}}"#
        )?
        .is_none()
    );
    Ok(())
}
#[test]
fn malformed_and_oversized_documents_fail_without_leaking_their_contents() {
    for text in [
        "{\"sops\": fixture-sensitive-malformed",
        "sops: [",
        "sops:\n  mac: ENC[mac]\n",
        "root: &a [*a]\nsops: {}\n",
    ] {
        let error = inspect("secret.yaml", text).err().unwrap_or_default();
        assert!(!error.is_empty());
        assert!(!error.contains("fixture-sensitive"));
    }
    assert!(inspect("secret.json", &"a".repeat(model::MAX_BYTES + 1)).is_err());
    for bytes in [
        vec![0],
        vec![0xff],
        vec![b'a'; model::MAX_BYTES + 1],
        "line\n".repeat(model::MAX_LINES + 1).into_bytes(),
        vec![b'a'; 8193],
    ] {
        assert!(tool::plaintext(bytes).is_err());
    }
}
#[test]
fn creation_deletion_and_plaintext_to_encrypted_transitions_use_the_correct_sides()
-> Result<(), String> {
    let encrypted = json("age1fixture", "cipher");
    let mut calls = 0;
    let view = compare("secret.json", "", &encrypted, &|| true, |_, input| {
        calls += 1;
        assert_eq!(input, encrypted);
        Ok("fixture plaintext\n".into())
    })?;
    assert_eq!(calls, 1);
    assert!(view.content_changed && view.recipients_changed);
    let view = compare("secret.json", &encrypted, "", &|| true, |_, _| {
        Ok("fixture plaintext\n".into())
    })?;
    assert!(
        view.rows
            .iter()
            .any(|r| r.kind == similar::ChangeTag::Delete)
    );
    let view = compare(
        "secret.json",
        r#"{"secret":"fixture"}"#,
        &encrypted,
        &|| true,
        |_, _| Ok(r#"{"secret":"fixture"}"#.into()),
    )?;
    assert!(!view.content_changed && view.recipients_changed);
    Ok(())
}
#[test]
fn close_reload_drop_and_late_answers_discard_private_results() -> Result<(), String> {
    let mut state = State::default();
    let (send, active) = state.begin();
    let result = plaintext_compare(
        &json("a", "one"),
        &json("b", "one"),
        "fixture plaintext",
        "fixture plaintext",
    )?;
    assert!(send.send(Ok(result)).is_ok());
    state.poll();
    assert!(state.result.is_some());
    state.close();
    assert!(!state.enabled && state.result.is_none() && state.receiver.is_none());
    assert!(!active.load(Ordering::Acquire));
    let (late, old_active) = state.begin();
    let (_current, current_active) = state.begin();
    assert!(!old_active.load(Ordering::Acquire));
    assert!(late.send(Err("late result".into())).is_err());
    assert!(current_active.load(Ordering::Acquire));
    drop(state);
    assert!(!current_active.load(Ordering::Acquire));
    let key = DiffKey::Work(crate::git::StatusFile {
        path: "secret.json".into(),
        old_path: None,
        area: crate::git::Area::Untracked,
        code: '?',
    });
    let mut view = crate::repo::DiffView::new(key);
    let (_, active) = view.encrypted.begin();
    view.next_request();
    assert!(!active.load(Ordering::Acquire));
    assert!(!view.encrypted.enabled);
    Ok(())
}
#[test]
fn cancellation_between_sides_never_returns_or_decrypts_a_second_plaintext() {
    let active = AtomicBool::new(true);
    let mut calls = 0;
    let result = compare(
        "secret.json",
        &json("a", "one"),
        &json("a", "two"),
        &|| active.load(Ordering::Acquire),
        |_, _| {
            calls += 1;
            active.store(false, Ordering::Release);
            Ok("fixture plaintext".into())
        },
    );
    assert!(result.is_err());
    assert_eq!(calls, 1);
}

#[test]
fn private_text_layout_does_not_enter_eguis_shared_galley_cache() {
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let mut counts = None;
    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
        egui::CentralPanel::default().show(ui, |ui| {
            let before = ui.fonts_mut(|fonts| fonts.num_galleys_in_cache());
            let galley = super::view::private_galley(
                ui,
                "fixture-private-text-that-must-not-be-cached".into(),
                egui::Color32::WHITE,
            );
            let after = ui.fonts_mut(|fonts| fonts.num_galleys_in_cache());
            counts = Some((before, after));
            ui.painter()
                .galley(egui::pos2(5.0, 5.0), galley, egui::Color32::WHITE);
        });
    });
    assert!(matches!(counts,Some((before,after)) if before==after));
    assert!(output.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Text(text) if text.galley.text().contains("fixture-private-text"))));
    output.shapes.clear();
    output.textures_delta.clear();
}

#[cfg(unix)]
mod fixture {
    use super::*;
    use crate::git::{self, Area, StatusFile};
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;
    use std::process::Command;
    use std::sync::atomic::AtomicU64;
    struct Repo(PathBuf);
    impl Repo {
        fn new() -> Result<Self, String> {
            static SEQ: AtomicU64 = AtomicU64::new(0);
            let root = std::env::temp_dir().join(format!(
                "gitcraft-sops ü 'fixture'-{}-{}",
                std::process::id(),
                SEQ.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
            let repo = Self(root);
            for args in [
                vec!["init", "-q"],
                vec!["config", "user.name", "Fixture"],
                vec!["config", "user.email", "fixture@example.invalid"],
                vec!["config", "commit.gpgSign", "false"],
            ] {
                repo.git(&args)?;
            }
            Ok(repo)
        }
        fn git(&self, args: &[&str]) -> Result<String, String> {
            git::practice_run(&self.0, args)
        }
        fn write(&self, path: &str, text: &str) -> Result<(), String> {
            std::fs::write(self.0.join(path), text).map_err(|e| e.to_string())
        }
        fn commit(&self) -> Result<String, String> {
            self.git(&["add", "-A"])?;
            self.git(&["commit", "-qm", "fixture"])?;
            self.git(&["rev-parse", "HEAD"])
                .map(|s| s.trim().to_owned())
        }
    }
    impl Drop for Repo {
        fn drop(&mut self) {
            tool::TEST_PROGRAM.with(|p| *p.borrow_mut() = None);
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn work(path: &str, area: Area, code: char) -> DiffKey {
        DiffKey::Work(StatusFile {
            path: path.into(),
            old_path: None,
            area,
            code,
        })
    }
    fn program(repo: &Repo, body: &str) -> Result<PathBuf, String> {
        let tool = repo.0.join("sops fixture 'tool'");
        std::fs::write(&tool, format!("#!/bin/sh\n{body}\n")).map_err(|e| e.to_string())?;
        std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;
        tool::TEST_PROGRAM.with(|p| *p.borrow_mut() = Some(tool.clone()));
        Ok(tool)
    }

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
        assert!(
            tab.prepare_picked(crate::repo::diff::Pick::Stage, &lines)
                .is_none()
        );
        if let Some(view) = &mut tab.diff {
            view.encrypted.close();
            view.next_request();
        }
        assert!(
            tab.prepare_picked(crate::repo::diff::Pick::Stage, &lines)
                .is_none()
        );
        assert_eq!(
            repo.git(&["show", &format!(":{path}")])?.trim(),
            json("age1fixture", "cipher1")
        );
        Ok(())
    }

    #[test]
    fn remote_sops_request_preserves_stdin_and_fixed_argv_without_plaintext_files()
    -> Result<(), String> {
        let repo = Repo::new()?;
        let executable = program(
            &repo,
            "test \"$#\" = 7 || exit 21; test \"$1 $2 $3 $4 $5 $6 $7\" = 'decrypt --input-type json --output-type json --filename-override gitcraft.json' || exit 22; input=$(cat); case \"$input\" in *'ENC[cipher]'*) printf fixture-private-remote;; *) exit 23;; esac",
        )?;
        let bin = repo.0.join("bin");
        std::fs::create_dir_all(&bin).map_err(|e| e.to_string())?;
        std::os::unix::fs::symlink(executable, bin.join("sops")).map_err(|e| e.to_string())?;
        let target = crate::remote::Target {
            host: crate::remote::Host::Ssh("fixture.invalid".into()),
            path: repo.0.to_string_lossy().into_owned(),
        };
        let request = tool::request(&target, Format::Json, &json("age1fixture", "cipher"));
        let mut command = Command::new("sh");
        command
            .args(["-c", &request.script])
            .current_dir(&repo.0)
            .env(
                "PATH",
                std::env::join_paths([bin, PathBuf::from("/usr/bin"), PathBuf::from("/bin")])
                    .map_err(|e| e.to_string())?,
            );
        let out = crate::bounded_process::run_input(
            command,
            request.stdin.clone(),
            &|| true,
            Duration::from_secs(2),
            1024,
            1024,
        )?;
        assert!(out.status.success());
        assert_eq!(out.stdout, b"fixture-private-remote");
        assert!(
            !request.script.contains("cipher")
                && !request.script.contains("mktemp")
                && !request.script.contains("fixture-private-remote")
        );
        Ok(())
    }

    #[test]
    fn local_cli_uses_only_ciphertext_stdin_and_fixed_safe_format_argv() -> Result<(), String> {
        let repo = Repo::new()?;
        program(
            &repo,
            r#"
 test "$#" = 7 || exit 11
 test "$1 $2 $3 $4 $5 $6 $7" = 'decrypt --input-type json --output-type json --filename-override gitcraft.json' || exit 12
 input=$(cat)
 case "$input" in *'ENC[cipher1]'*) printf 'fixture-old-private\n';; *'ENC[cipher2]'*) printf 'fixture-new-private\n';; *) exit 13;; esac
"#,
        )?;
        let path = "secret '$(touch INJECTED)' ü.json";
        let old = json("age1fixture", "cipher1");
        let new = json("age1fixture", "cipher2");
        repo.write(path, &old)?;
        repo.commit()?;
        repo.write(path, &new)?;
        let key = work(path, Area::Unstaged, 'M');
        let result = load(&repo.0, &key, &|| true)?;
        assert!(result.content_changed && !result.recipients_changed);
        assert_eq!(
            std::fs::read_to_string(repo.0.join(path)).map_err(|e| e.to_string())?,
            new
        );
        assert_eq!(repo.git(&["show", &format!(":{path}")])?.trim(), old);
        assert!(!repo.0.join("INJECTED").exists());
        let data = crate::repo::diff::load(&repo.0, &key, git::DiffOptions::default(), true)?;
        assert!(data.sops && data.partial);
        let hunk = data
            .lines
            .iter()
            .position(|l| l.kind == crate::repo::diff::Kind::Hunk)
            .ok_or("No encrypted hunk")?;
        let patch = crate::repo::diff::partial_patch(
            &data.header,
            &data.lines,
            &crate::repo::diff::hunk_changes(&data.lines, hunk),
            crate::repo::diff::Pick::Stage,
        )
        .ok_or("No encrypted patch")?;
        assert!(patch.contains("ENC[cipher2]"));
        assert!(!patch.contains("fixture-new-private"));
        assert!(
            data.lines
                .iter()
                .all(|line| !line.text.contains("fixture-new-private"))
        );
        assert!(repo.0.read_dir().map_err(|e| e.to_string())?.all(|entry| {
            entry.is_ok_and(|e| e.file_name() != "before" && e.file_name() != "after")
        }));
        Ok(())
    }

    #[test]
    fn plaintext_tool_diagnostics_missing_tools_binary_and_oversized_output_are_redacted()
    -> Result<(), String> {
        let repo = Repo::new()?;
        let input = json("age1fixture", "cipher1");
        program(
            &repo,
            "cat >/dev/null; printf 'fixture-private-stdout'; printf 'fixture-private-stderr' >&2; exit 1",
        )?;
        let error = tool::decrypt(&repo.0, Format::Json, &input, &|| true)
            .err()
            .ok_or("Expected tool failure")?;
        assert!(!error.contains("fixture-private"));
        assert!(error.contains("keys/permissions"));
        tool::TEST_PROGRAM.with(|p| *p.borrow_mut() = Some(repo.0.join("not-installed")));
        assert!(
            tool::decrypt(&repo.0, Format::Json, &input, &|| true)
                .is_err_and(|e| e.contains("Install SOPS"))
        );
        program(&repo, "cat >/dev/null; printf '\\377'")?;
        assert!(
            tool::decrypt(&repo.0, Format::Json, &input, &|| true)
                .is_err_and(|e| e.contains("UTF-8"))
        );
        program(&repo, "cat >/dev/null; head -c 2097153 /dev/zero")?;
        assert!(
            tool::decrypt(&repo.0, Format::Json, &input, &|| true)
                .is_err_and(|e| e.contains("size limit"))
        );
        program(&repo, "cat >/dev/null; head -c 32769 /dev/zero >&2")?;
        assert!(
            tool::decrypt(&repo.0, Format::Json, &input, &|| true)
                .is_err_and(|e| e.contains("size limit"))
        );
        Ok(())
    }

    #[test]
    fn cancellation_kills_local_decryption_and_late_descendants_without_disk_output()
    -> Result<(), String> {
        let repo = Repo::new()?;
        let marker = repo.0.join("descendant-reached");
        program(
            &repo,
            &format!(
                "cat >/dev/null; (sleep 0.5; printf leaked > {}) & wait",
                crate::remote::sh_quote(&marker.to_string_lossy())
            ),
        )?;
        let start = std::time::Instant::now();
        assert!(
            tool::decrypt(
                &repo.0,
                Format::Json,
                &json("age1fixture", "cipher"),
                &|| start.elapsed() < Duration::from_millis(100)
            )
            .is_err_and(|e| e.contains("cancelled"))
        );
        assert!(start.elapsed() < Duration::from_secs(2));
        std::thread::sleep(Duration::from_millis(600));
        assert!(!marker.exists());
        Ok(())
    }

    #[test]
    fn changed_encrypted_sides_discard_plaintext_before_returning_a_result() -> Result<(), String> {
        let repo = Repo::new()?;
        let path = "secret.json";
        repo.write(path, &json("age1fixture", "cipher1"))?;
        repo.commit()?;
        repo.write(path, &json("age1fixture", "cipher2"))?;
        // Modifying the fixture input simulates an edit while the decrypt subprocess runs.
        program(
            &repo,
            &format!(
                "cat >/dev/null; printf '%s' {} > {}; printf 'fixture-private-output'",
                crate::remote::sh_quote(&json("age1fixture", "cipher3")),
                crate::remote::sh_quote(path)
            ),
        )?;
        let result = load(&repo.0, &work(path, Area::Unstaged, 'M'), &|| true);
        assert!(result.is_err_and(
            |e| e.contains("changed while decrypting") && !e.contains("fixture-private")
        ));
        Ok(())
    }

    #[test]
    fn commit_index_worktree_and_extensionless_sops_are_recognized_without_decrypting()
    -> Result<(), String> {
        let repo = Repo::new()?;
        let path = "encrypted-config";
        let old = json("age1first", "cipher1");
        let indexed = json("age1second", "cipher2");
        let working = json("age1third", "cipher3");
        repo.write(path, &old)?;
        let root = repo.commit()?;
        repo.write(path, &indexed)?;
        repo.git(&["add", "--", path])?;
        repo.write(path, &working)?;
        assert_eq!(
            read_sides(&repo.0, &work(path, Area::Staged, 'M'))?,
            (old.clone(), indexed)
        );
        assert_eq!(
            read_sides(
                &repo.0,
                &DiffKey::Commit {
                    hash: root,
                    path: path.into()
                }
            )?,
            (String::new(), old)
        );
        assert!(probe(&repo.0, &work(path, Area::Unstaged, 'M')));
        assert!(
            crate::repo::diff::load(
                &repo.0,
                &work(path, Area::Unstaged, 'M'),
                git::DiffOptions::default(),
                true
            )?
            .sops
        );
        Ok(())
    }

    #[test]
    fn git_textconv_cannot_decrypt_ordinary_patches_or_run_before_opt_in() -> Result<(), String> {
        let repo = Repo::new()?;
        let path = "secret.json";
        let driver = repo.0.join("decrypting-textconv");
        let marker = repo.0.join("textconv-ran");
        std::fs::write(
            &driver,
            format!(
                "#!/bin/sh\nprintf ran > {}; printf 'fixture-private-conversion'\n",
                crate::remote::sh_quote(&marker.to_string_lossy())
            ),
        )
        .map_err(|e| e.to_string())?;
        std::fs::set_permissions(&driver, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;
        repo.write(".gitattributes", "secret.json diff=sops-fixture\n")?;
        repo.git(&[
            "config",
            "diff.sops-fixture.textconv",
            &driver.to_string_lossy(),
        ])?;
        repo.write(path, &json("age1fixture", "cipher1"))?;
        let first = repo.commit()?;
        repo.write(path, &json("age1fixture", "cipher2"))?;
        for key in [
            work(path, Area::Unstaged, 'M'),
            DiffKey::Range {
                from: first,
                to: None,
                path: path.into(),
            },
        ] {
            let data = crate::repo::diff::load(&repo.0, &key, git::DiffOptions::default(), true)?;
            assert!(data.sops);
            assert!(!marker.exists());
            assert!(
                data.lines
                    .iter()
                    .all(|l| !l.text.contains("fixture-private-conversion"))
            );
        }
        Ok(())
    }

    #[test]
    fn decrypted_ui_is_read_only_and_mode_close_reload_or_park_disposes_it() -> Result<(), String> {
        let repo = Repo::new()?;
        let path = "secret.json";
        repo.write(path, &json("age1fixture", "cipher1"))?;
        repo.commit()?;
        repo.write(path, &json("age1fixture", "cipher2"))?;
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let mut tab = crate::repo::RepoTab::new_deferred(repo.0.clone(), &ctx);
        let make_view = || -> Result<crate::repo::DiffView, String> {
            let mut view = crate::repo::DiffView::new(work(path, Area::Unstaged, 'M'));
            view.next_request();
            view.data = Some(crate::repo::diff::load(
                &repo.0,
                &view.key,
                git::DiffOptions::default(),
                true,
            ));
            let (send, _) = view.encrypted.begin();
            send.send(plaintext_compare(
                &json("a", "x"),
                &json("b", "y"),
                "fixture-private-old",
                "fixture-private-new",
            ))
            .map_err(|_| "No private receiver")?;
            view.encrypted.poll();
            Ok(view)
        };
        tab.diff = Some(make_view()?);
        let mut actions = Vec::new();
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
        assert!(text.contains("fixture-private-new"));
        assert!(
            !text.contains("Stage file")
                && !text.contains("Stage hunk")
                && !text.contains("Stage 1 line")
        );
        assert!(actions.is_empty());
        out.shapes.clear();
        out.textures_delta.clear();
        for action in [
            crate::repo::Action::DiffAsFile(false),
            crate::repo::Action::DiffRendered(false),
            crate::repo::Action::DiffStructural(false),
            crate::repo::Action::CloseDiff,
        ] {
            tab.diff = Some(make_view()?);
            let active = tab
                .diff
                .as_ref()
                .and_then(|v| v.encrypted.active.clone())
                .ok_or("No active token")?;
            tab.apply(action, &ctx);
            assert!(!active.load(Ordering::Acquire));
        }
        tab.diff = Some(make_view()?);
        tab.poll_parked_view(&ctx);
        assert!(tab.diff.as_ref().is_some_and(|v| !v.encrypted.enabled));
        Ok(())
    }
}

#[test]
fn cancelled_slow_ciphertext_reads_do_not_hold_the_plaintext_owner_waiting() {
    let start = std::time::Instant::now();
    let result = await_ciphertext(
        || {
            std::thread::sleep(Duration::from_millis(200));
            Ok(("ciphertext-only".into(), "ciphertext-only".into()))
        },
        &|| start.elapsed() < Duration::from_millis(30),
    );
    assert!(result.is_err_and(|e| e.contains("cancelled")));
    assert!(start.elapsed() < Duration::from_millis(150));
}
