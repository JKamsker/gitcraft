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
            .any(|row| row.kind == crate::repo::diff::line_diff::Edit::Del
                && row.text == "password: old")
    );
    assert!(
        view.rows
            .iter()
            .any(|row| row.kind == crate::repo::diff::line_diff::Edit::Add
                && row.text == "password: new")
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
        Some(Format::Json)
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
fn data_envelopes_require_explicit_format_independent_of_filename() -> Result<(), String> {
    let envelope = r#"{"data":"ENC[cipher]","sops":{"version":"3.10.0","mac":"ENC[mac]"}}"#;
    for path in ["config", "config.yaml", "config.json", "config.bin"] {
        assert!(inspect(path, envelope)?.is_some_and(|e| e.ambiguous_json));
        let mut calls = 0;
        let refused = compare(path, envelope, envelope, &|| true, |_, _| {
            calls += 1;
            Err("Ambiguous detection must not invoke SOPS".to_owned())
        });
        assert_eq!(calls, 0);
        assert!(refused.is_err_and(|e| e.contains("Choose JSON document or Binary envelope")));
        for (encoding, format, plaintext) in [
            (JsonEncoding::Document, Format::Json, r#"{"data":42}"#),
            (JsonEncoding::Binary, Format::Binary, "actual text\n"),
        ] {
            let result = compare_with_encoding(
                path,
                envelope,
                envelope,
                encoding,
                &|| true,
                |selected, _| {
                    assert_eq!(selected, format);
                    Ok(plaintext.to_owned())
                },
            )?;
            assert!(!result.content_changed);
            assert!(result.rows.iter().any(|r| r.text.contains(
                if encoding == JsonEncoding::Binary {
                    "actual text"
                } else {
                    "42"
                }
            )));
        }
        assert!(
            compare_with_encoding(
                path,
                envelope,
                envelope,
                JsonEncoding::Document,
                &|| true,
                |_, _| Ok(r#"{"data":"binary\u0000content"}"#.to_owned())
            )
            .is_err()
        );
    }
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
            .any(|r| r.kind == crate::repo::diff::line_diff::Edit::Del)
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
            let wrapped = super::view::private_wrapped_galley(
                ui,
                "fixture-private-text-that-must-not-be-cached".repeat(10),
                egui::Color32::WHITE,
                Some(100.),
            );
            assert!(wrapped.rows.len() > 3);
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
#[path = "tests/fixture.rs"]
mod fixture;

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
