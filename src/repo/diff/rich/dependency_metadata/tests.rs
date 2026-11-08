use super::*;
use std::io::Write;

fn archive(files: &[(&str, &str)]) -> Vec<u8> {
    let mut tar = Vec::new();
    for (name, body) in files {
        let mut header = [0u8; 512];
        let path = format!("package/{name}");
        header[..path.len()].copy_from_slice(path.as_bytes());
        header[124..136].copy_from_slice(format!("{:011o}\0", body.len()).as_bytes());
        header[148..156].fill(b' ');
        header[156] = b'0';
        let sum: usize = header.iter().map(|b| *b as usize).sum();
        header[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
        tar.extend_from_slice(&header);
        tar.extend_from_slice(body.as_bytes());
        tar.resize(tar.len().div_ceil(512) * 512, 0);
    }
    tar.extend_from_slice(&[0u8; 1024]);
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    assert!(gz.write_all(&tar).is_ok());
    gz.finish().unwrap_or_default()
}
#[test]
fn exact_published_versions_reveal_real_license_change_and_changelog() -> Result<(), String> {
    let a = archive(&[
        (
            "Cargo.toml",
            "[package]\nname='thing'\nversion='1.0.0'\nlicense='MIT'",
        ),
        ("CHANGELOG.md", "# 1.0.0\nFirst release"),
        ("src/lib.rs", "do not execute"),
    ]);
    let b = archive(&[
        (
            "Cargo.toml",
            "[package]\nname='thing'\nversion='2.0.0'\nlicense='Apache-2.0'",
        ),
        ("CHANGES", "Breaking change"),
    ]);
    let old = parse(Ecosystem::Cargo, "thing", "1.0.0", "old".into(), &a)?;
    let new = parse(Ecosystem::Cargo, "thing", "2.0.0", "new".into(), &b)?;
    assert_eq!(old.license.as_deref(), Some("MIT"));
    assert_eq!(new.license.as_deref(), Some("Apache-2.0"));
    assert_eq!(old.changelogs.len(), 1);
    assert_eq!(new.changelogs["CHANGES"], "Breaking change");
    assert!(parse(Ecosystem::Cargo, "other", "1.0.0", String::new(), &a).is_err());
    Ok(())
}
#[test]
fn npm_identity_missing_license_and_unavailable_changelog_are_not_invented() -> Result<(), String> {
    let data = archive(&[("package.json", r#"{"name":"@org/pkg","version":"1.0.0"}"#)]);
    let result = parse(Ecosystem::Npm, "@org/pkg", "1.0.0", String::new(), &data)?;
    assert_eq!(result.license, None);
    assert!(result.changelogs.is_empty());
    assert_eq!(
        urls(Ecosystem::Npm, "@org/pkg", "1.0.0")?.0.as_deref(),
        Some("https://registry.npmjs.org/@org%2Fpkg/1.0.0")
    );
    for name in ["../p", "@org/../secret", "x?y", "https://x", "@o/p/z"] {
        assert!(urls(Ecosystem::Npm, name, "1.0.0").is_err());
    }
    assert!(urls(Ecosystem::Cargo, "thing", "../x").is_err());
    Ok(())
}
#[test]
fn bounded_archive_parser_refuses_traversal_duplicate_corrupt_and_bomb() {
    assert!(archive::texts(&archive(&[("../CHANGELOG.md", "secret")])).is_err());
    assert!(archive::texts(&archive(&[("CHANGELOG.md", "a"), ("CHANGELOG.md", "b")])).is_err());
    assert!(archive::texts(b"not gzip").is_err());
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    assert!(gz.write_all(&vec![0u8; (32 << 20) + 1]).is_ok());
    assert!(archive::texts(&gz.finish().unwrap_or_default()).is_err());
}
#[test]
fn private_registry_packages_do_not_leak_to_public_metadata_lookup() -> Result<(), String> {
    let packages = super::super::deps::packages(
        "Cargo.lock",
        "[[package]]\nname='private'\nversion='1.0.0'\nsource='registry+https://private.example/index'",
    )?;
    assert!(packages[0].registry.is_none());
    let packages = super::super::deps::packages(
        "package-lock.json",
        r#"{"packages":{"node_modules/private":{"version":"1.0.0","resolved":"https://private.example/private.tgz"}}}"#,
    )?;
    assert!(packages[0].registry.is_none());
    Ok(())
}

#[test]
fn http_redirect_and_oversize_are_refused_without_following() -> Result<(), String> {
    use std::net::TcpListener;
    for (status, body) in [("302 Found", vec![]), ("200 OK", vec![b'x'; (8 << 20) + 1])] {
        let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
        let address = listener.local_addr().map_err(|e| e.to_string())?;
        let worker = std::thread::spawn(move || {
            let Ok((mut socket, _)) = listener.accept() else {
                return;
            };
            let mut request = [0; 4096];
            let _ = socket.read(&mut request);
            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nLocation: http://127.0.0.1:1/leak\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = socket.write_all(response.as_bytes());
            let _ = socket.write_all(&body);
        });
        assert!(get(&format!("http://{address}/package")).is_err());
        worker.join().map_err(|_| "fixture worker panicked")?;
    }
    Ok(())
}

#[test]
fn details_panel_reports_version_licenses_and_actual_changelog_presence() {
    let mut state = State {
        selected: Some((
            "thing".into(),
            Request {
                ecosystem: Ecosystem::Cargo,
                old: None,
                new: None,
            },
        )),
        result: Some((
            Ok(Some(Metadata {
                license: Some("MIT".into()),
                changelogs: BTreeMap::new(),
                archive_url: "https://static.crates.io/old.crate".into(),
                ..Metadata::default()
            })),
            Ok(Some(Metadata {
                license: Some("Apache-2.0".into()),
                changelogs: BTreeMap::from([(
                    "CHANGELOG.md".into(),
                    "Actual release notes".into(),
                )]),
                archive_url: "https://static.crates.io/new.crate".into(),
                ..Metadata::default()
            })),
        )),
        ..State::default()
    };
    let ctx = egui::Context::default();
    let output = ctx.run_ui(egui::RawInput::default(), |ui| state.show(ui));
    let text = output
        .shapes
        .iter()
        .filter_map(|s| {
            if let egui::Shape::Text(t) = &s.shape {
                Some(t.galley.text())
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("Licence changed: MIT → Apache-2.0"), "{text}");
    assert!(text.contains("changelog unavailable"), "{text}");
    assert!(text.contains("After changelog: CHANGELOG.md"), "{text}");
    output.drop_without_applying_deltas();
}

#[test]
fn declared_licence_files_and_legacy_npm_objects_remain_evidence() -> Result<(), String> {
    let manifest = "[package]\nname='thing'\nversion='1.0.0'\nlicense-file='legal/terms.txt'";
    let a = parse(
        Ecosystem::Cargo,
        "thing",
        "1.0.0",
        "old".into(),
        &archive(&[
            ("Cargo.toml", manifest),
            ("legal/terms.txt", "Custom licence version one"),
        ]),
    )?;
    let b = parse(
        Ecosystem::Cargo,
        "thing",
        "1.0.0",
        "new".into(),
        &archive(&[
            ("Cargo.toml", manifest),
            ("legal/terms.txt", "Custom licence version two"),
        ]),
    )?;
    assert_eq!(a.license, None);
    assert_eq!(
        a.license_file.as_ref().map(|(p, _)| p.as_str()),
        Some("legal/terms.txt")
    );
    assert!(
        a.license_summary()
            .contains("declared file legal/terms.txt")
    );
    assert!(changed(&(Ok(Some(a.clone())), Ok(Some(b)))));
    let missing = parse(
        Ecosystem::Cargo,
        "thing",
        "1.0.0",
        "missing".into(),
        &archive(&[("Cargo.toml", manifest)]),
    )?;
    assert!(!missing.license_complete());
    assert!(!changed(&(Ok(Some(a)), Ok(Some(missing)))));
    let npm = parse(
        Ecosystem::Npm,
        "pkg",
        "1.0.0",
        "npm".into(),
        &archive(&[(
            "package.json",
            r#"{"name":"pkg","version":"1.0.0","license":{"type":"BSD-2-Clause","url":"https://example.invalid/licence"}}"#,
        )]),
    )?;
    assert_eq!(npm.license.as_deref(), Some("BSD-2-Clause"));
    let npm = parse(
        Ecosystem::Npm,
        "pkg",
        "1.0.0",
        "npm".into(),
        &archive(&[(
            "package.json",
            r#"{"name":"pkg","version":"1.0.0","licenses":[{"type":"MIT"},{"type":"ISC"}]}"#,
        )]),
    )?;
    assert_eq!(npm.license.as_deref(), Some("MIT OR ISC"));
    Ok(())
}
#[test]
fn oversized_changelog_and_licence_file_are_included_but_explicitly_unreadable()
-> Result<(), String> {
    let huge = "x".repeat((512 << 10) + 1);
    let bytes = archive(&[
        (
            "Cargo.toml",
            "[package]\nname='thing'\nversion='1.0.0'\nlicense-file='LICENSE'",
        ),
        ("CHANGELOG.md", &huge),
        ("LICENSE", &huge),
    ]);
    let m = parse(Ecosystem::Cargo, "thing", "1.0.0", "source".into(), &bytes)?;
    assert!(m.changelogs.is_empty());
    assert!(m.skipped["CHANGELOG.md"].contains("included, but exceeds"));
    assert!(
        m.license_file
            .as_ref()
            .is_some_and(|(_, text)| text.as_ref().is_err_and(|e| e.contains("512 KiB")))
    );
    assert!(!m.license_complete());
    let mut doc = crate::repo::find::rendered::Document::default();
    m.append_document(
        &mut doc,
        "After",
        &crate::theme::Tokens::get(&egui::Context::default()),
    );
    let text = doc
        .rows
        .iter()
        .map(|r| r.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("CHANGELOG.md · included, but exceeds"));
    assert!(!text.contains("not included in the published package"));
    Ok(())
}
fn wait(state: &mut State, ctx: &egui::Context) {
    let until = std::time::Instant::now() + Duration::from_secs(5);
    while state.pending.is_some() && std::time::Instant::now() < until {
        state.poll(ctx);
        std::thread::yield_now();
    }
    assert!(
        state.pending.is_none(),
        "bounded metadata worker did not reply"
    );
}
#[test]
fn independent_sides_survive_failure_and_reselection_after_close_retries() {
    use std::sync::{Arc, Mutex};
    let request = Request {
        ecosystem: Ecosystem::Cargo,
        old: Some(("thing".into(), "1.0.0".into())),
        new: Some(("thing".into(), "2.0.0".into())),
    };
    let ctx = egui::Context::default();
    let mut state = State::default();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let recorded = Arc::clone(&calls);
    state.start_using("thing".into(), request.clone(), &ctx, false, move |_, r| {
        let version = r.as_ref().map(|(_, v)| v.clone()).unwrap_or_default();
        recorded
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(version.clone());
        if version == "1.0.0" {
            Err("temporary old registry error".into())
        } else {
            Ok(Some(Metadata {
                license: Some("MIT".into()),
                ..Metadata::default()
            }))
        }
    });
    wait(&mut state, &ctx);
    assert_eq!(
        *calls
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
        vec!["1.0.0", "2.0.0"]
    );
    assert!(state.cache.is_empty());
    assert!(
        state
            .result
            .as_ref()
            .is_some_and(|(a, b)| a.is_err() && matches!(b, Ok(Some(_))))
    );
    assert!(!changed(
        state.result.as_ref().unwrap_or(&(Ok(None), Ok(None)))
    ));
    let output = ctx.run_ui(egui::RawInput::default(), |ui| state.show(ui));
    let text = output
        .shapes
        .iter()
        .filter_map(|s| {
            if let egui::Shape::Text(t) = &s.shape {
                Some(t.galley.text())
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("Retry metadata"));
    assert!(text.contains("temporary old registry error"));
    assert!(text.contains("MIT"));
    output.drop_without_applying_deltas();
    state.close();
    let recorded = Arc::clone(&calls);
    state.start_using("thing".into(), request, &ctx, false, move |_, r| {
        recorded
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(r.as_ref().map(|(_, v)| v.clone()).unwrap_or_default());
        Ok(Some(Metadata {
            license: Some("MIT".into()),
            ..Metadata::default()
        }))
    });
    wait(&mut state, &ctx);
    assert_eq!(
        calls
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len(),
        4
    );
    assert_eq!(state.cache.len(), 1);
    assert!(
        state
            .result
            .as_ref()
            .is_some_and(|(a, b)| a.is_ok() && b.is_ok())
    );
    let answer = read_both(
        Ecosystem::Cargo,
        &Some(("good".into(), "1".into())),
        &Some(("broken".into(), "2".into())),
        |_, r| {
            if r.as_ref().is_some_and(|(name, _)| name == "broken") {
                Err("new failed".into())
            } else {
                Ok(Some(Metadata {
                    license: Some("ISC".into()),
                    ..Metadata::default()
                }))
            }
        },
    );
    assert!(answer.0.is_ok() && answer.1.is_err());
}

#[test]
fn accepted_metadata_arrival_rebuilds_find_while_reader_stays_open() -> Result<(), String> {
    use super::super::{Rich, RichDiff, RichKind, deps};
    use crate::repo::{DiffKey, DiffView, RepoTab};
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let mut tab = RepoTab::new_deferred("/nonexistent/metadata-find".into(), &ctx);
    let mut view = DiffView::new(DiffKey::Commit {
        hash: "abc".into(),
        path: "Cargo.lock".into(),
    });
    view.rich = Some((
        RichKind::Deps,
        Ok(Rich::Ready(RichDiff::Deps(deps::DepsDiff {
            ecosystem: Ecosystem::Cargo,
            file: "Cargo.lock",
            changes: Vec::new(),
        }))),
    ));
    let (tx, rx) = mpsc::channel();
    view.rich_state.metadata.selected = Some((
        "thing".into(),
        Request {
            ecosystem: Ecosystem::Cargo,
            old: None,
            new: Some(("thing".into(), "2.0.0".into())),
        },
    ));
    view.rich_state.metadata.pending = Some(Pending {
        rx,
        _scope: tasks::Scope::default(),
        context: connections::current(),
    });
    let request = view.request;
    tab.diff = Some(view);
    tab.diff_rich = Some(RichKind::Deps);
    tab.find.show();
    tab.find.query.text = "release_note_needle".into();
    let out = ctx.run_ui(egui::RawInput::default(), |ui| {
        tab.diff_view(ui, &mut Vec::new())
    });
    out.drop_without_applying_deltas();
    assert_eq!(tab.find.count(), 0);
    tx.send((
        Err("old unavailable".into()),
        Ok(Some(Metadata {
            license: Some("CustomLicense".into()),
            license_file: Some(("LICENSE".into(), Ok("custom terms needle".into()))),
            changelogs: BTreeMap::from([("CHANGES".into(), "release_note_needle".into())]),
            archive_url: "https://static.crates.io/new.crate".into(),
            ..Metadata::default()
        })),
    ))
    .map_err(|_| "metadata reply failed")?;
    let out = ctx.run_ui(egui::RawInput::default(), |ui| {
        tab.diff_view(ui, &mut Vec::new())
    });
    out.drop_without_applying_deltas();
    assert_eq!(tab.diff.as_ref().map(|v| v.request), Some(request));
    assert_eq!(
        tab.find.count(),
        1,
        "async metadata must join the already-open reading document"
    );
    for query in ["CustomLicense", "custom terms needle", "old unavailable"] {
        tab.find.query.text = query.into();
        let out = ctx.run_ui(egui::RawInput::default(), |ui| {
            tab.diff_view(ui, &mut Vec::new())
        });
        out.drop_without_applying_deltas();
        assert!(tab.find.count() > 0, "missing metadata text: {query}");
    }
    Ok(())
}
