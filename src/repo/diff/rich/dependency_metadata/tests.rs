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
        result: Some(Ok((
            Some(Metadata {
                license: Some("MIT".into()),
                changelogs: BTreeMap::new(),
                archive_url: "https://static.crates.io/old.crate".into(),
            }),
            Some(Metadata {
                license: Some("Apache-2.0".into()),
                changelogs: BTreeMap::from([(
                    "CHANGELOG.md".into(),
                    "Actual release notes".into(),
                )]),
                archive_url: "https://static.crates.io/new.crate".into(),
            }),
        ))),
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
