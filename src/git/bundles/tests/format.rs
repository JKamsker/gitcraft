//! Format.

use super::*;

#[test]
pub(super) fn preview_never_unpacks_even_a_corrupt_pack() -> Result<(), String> {
    let f = Fixture::new()?;
    let source = f.repo("inspection-source")?;
    f.commit(&source, "one")?;
    let mut bytes = export_bundle(&source, &[head(&source)?], None)?;
    let end = bytes
        .windows(2)
        .position(|b| b == b"\n\n")
        .ok_or("header")?
        + 2;
    bytes.truncate(end);
    bytes.extend_from_slice(b"PACK deliberately corrupt: inspection must not unpack");
    let dest = f.repo("inspection-dest")?;
    let before = run(Some(&dest), &["count-objects", "-v"])?;
    let preview = inspect_bundle(&dest, bytes)?;
    assert!(
        preview.verification_error.is_none(),
        "{:?}",
        preview.verification_error
    );
    assert_eq!(preview.refs.len(), 1);
    assert_eq!(run(Some(&dest), &["count-objects", "-v"])?, before);
    assert!(refs(&dest)?.is_empty());
    Ok(())
}

#[test]
pub(super) fn local_ref_validation_matches_native_git_without_remote_round_trips() {
    for name in [
        "refs/heads/main",
        "refs/tags/ü",
        "refs/heads/quote'$(literal)",
        "refs/heads/a@b",
        "refs/heads/has]bracket",
        "refs/heads/a.lock/x",
        "refs/heads/.hidden",
        "refs/heads/a..b",
        "refs/heads/a@{b",
        "refs/heads/a[",
        "refs/heads/a ",
        "refs/heads/a\\b",
        "refs/heads/a.",
        "refs//head",
        "refs/heads/x/",
        "refs/heads/a.lock",
    ] {
        assert_eq!(
            valid_ref(name).is_ok(),
            run(None, &["check-ref-format", name]).is_ok(),
            "{name}"
        );
    }
}

#[test]
pub(super) fn export_ref_snapshots_and_saved_files_cannot_be_overwritten() -> Result<(), String> {
    let f = Fixture::new()?;
    let source = f.repo("source")?;
    let oid = f.commit(&source, "one")?;
    let selected = head(&source)?;
    let bytes = export_bundle(&source, std::slice::from_ref(&selected), None)?;
    f.commit(&source, "two")?;
    assert!(export_bundle(&source, &[selected], None).is_err());
    let out = f.0.join("quoted ' output.bundle");
    save_bundle_file(&out, &bytes)?;
    assert!(save_bundle_file(&out, b"replace").is_err());
    let snapshot = read_bundle_file(&out)?;
    assert_eq!(snapshot, bytes);
    std::fs::write(&out, b"replaced after preview").map_err(|e| e.to_string())?;
    let dest = f.repo("dest")?;
    let preview = inspect_bundle(&dest, snapshot)?;
    assert_eq!(preview.refs[0].oid, oid);
    assert!(refs(&dest)?.is_empty());
    Ok(())
}

#[test]
pub(super) fn malformed_headers_and_untrusted_refs_are_rejected_without_unpacking()
-> Result<(), String> {
    let f = Fixture::new()?;
    let source = f.repo("source")?;
    f.commit(&source, "one")?;
    let bytes = export_bundle(&source, &[head(&source)?], None)?;
    let oid = head(&source)?.oid;
    let pack = &bytes[bytes
        .windows(2)
        .position(|s| s == b"\n\n")
        .ok_or("no pack")?
        + 2..];
    for description in ["", "\tTabbed subject"] {
        let mut sample =
            format!("# v2 git bundle\n-{oid}{description}\n{oid} HEAD\n\n").into_bytes();
        // A tabbed subject uses the normal space after the prerequisite OID.
        if !description.is_empty() {
            sample = format!("# v2 git bundle\n-{oid} {description}\n{oid} HEAD\n\n").into_bytes();
        }
        sample.extend(pack);
        let (_, prerequisites) = header(&sample)?;
        assert_eq!(prerequisites.len(), 1);
        assert!(!prerequisites[0].name.chars().any(char::is_control));
    }
    let dest = f.repo("dest")?;
    for bad in [
        b"not a bundle".to_vec(),
        b"# v2 git bundle\n\nPACK".to_vec(),
        bytes[..20].to_vec(),
    ] {
        assert!(inspect_bundle(&dest, bad).is_err());
    }
    for name in [
        "--output=evil",
        "refs/heads/a b",
        "refs/heads/../escape",
        "refs/heads/a\ncreate refs/heads/evil",
        "refs/heads/a.lock",
    ] {
        let bad = String::from_utf8_lossy(&bytes).replacen(" HEAD\n", &format!(" {name}\n"), 1);
        assert!(inspect_bundle(&dest, bad.into_bytes()).is_err(), "{name}");
    }
    assert!(refs(&dest)?.is_empty());
    Ok(())
}

#[test]
pub(super) fn unicode_whitespace_refs_and_legacy_prerequisite_subjects_are_preserved()
-> Result<(), String> {
    let f = Fixture::new()?;
    let source = f.repo("unicode-refs")?;
    let oid = f.commit(&source, "one")?;
    let name = "refs/heads/nonbreaking\u{a0}space";
    run(Some(&source), &["update-ref", name, &oid])?;
    let selected = bundle_refs(&source)?
        .into_iter()
        .find(|r| r.name == name)
        .ok_or("Unicode whitespace ref missing")?;
    let bytes = export_bundle(&source, &[selected], None)?;
    assert_eq!(header(&bytes)?.0[0].name, name);
    let sample = [
        format!("# v2 git bundle\n-{oid} legacy ").into_bytes(),
        vec![0xe9],
        format!("\n{oid} HEAD\n\nPACK").into_bytes(),
    ]
    .concat();
    let (_, prerequisites) = header(&sample)?;
    assert_eq!(prerequisites[0].oid, oid);
    assert!(prerequisites[0].name.starts_with("legacy "));
    let bad_ref = [
        format!("# v2 git bundle\n{oid} refs/heads/").into_bytes(),
        vec![0xe9],
        b"\n\nPACK".to_vec(),
    ]
    .concat();
    assert!(header(&bad_ref).is_err());
    Ok(())
}

#[cfg(unix)]
#[test]
pub(super) fn partial_clone_export_never_fetches_promised_blobs() -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new()?;
    let source = f.repo("offline-export-source")?;
    run(Some(&source), &["config", "uploadpack.allowFilter", "true"])?;
    run(
        Some(&source),
        &["config", "uploadpack.allowAnySHA1InWant", "true"],
    )?;
    f.commit(&source, "promised blob")?;
    let missing = run(Some(&source), &["rev-parse", "HEAD:file"])?;
    let dest = f.0.join("offline-export-dest");
    run(
        None,
        &[
            "-c",
            "protocol.file.allow=always",
            "clone",
            "--no-local",
            "--no-checkout",
            "--filter=blob:none",
            "--",
            source.to_str().ok_or("source")?,
            dest.to_str().ok_or("destination")?,
        ],
    )?;
    run(Some(&dest), &["config", "protocol.file.allow", "always"])?;
    let marker = f.0.join("export-fetch-ran");
    let helper = f.0.join("export-upload-pack");
    std::fs::write(
        &helper,
        format!(
            "#!/bin/sh\nprintf fetch >> {}\nexec git-upload-pack \"$@\"\n",
            remote::sh_quote(marker.to_str().ok_or("marker")?)
        ),
    )
    .map_err(|e| e.to_string())?;
    std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o700))
        .map_err(|e| e.to_string())?;
    run(
        Some(&dest),
        &[
            "config",
            "remote.origin.uploadpack",
            helper.to_str().ok_or("helper")?,
        ],
    )?;
    let before = run(Some(&dest), &["count-objects", "-v"])?;
    let selected = head(&dest)?;
    assert!(export_bundle(&dest, std::slice::from_ref(&selected), None).is_err());
    assert!(run(Some(&dest), &["cat-file", "-e", missing.trim()]).is_err());
    assert!(!marker.exists());
    assert_eq!(run(Some(&dest), &["count-objects", "-v"])?, before);
    // Only this explicit fetch may invoke the helper serving our local origin.
    run(
        Some(&dest),
        &[
            "fetch",
            "--no-tags",
            "--no-write-fetch-head",
            "origin",
            missing.trim(),
        ],
    )?;
    assert!(marker.exists(), "Explicit local fetch must call the helper");
    assert_ne!(run(Some(&dest), &["count-objects", "-v"])?, before);
    let calls = std::fs::read(&marker).map_err(|e| e.to_string())?;
    run(Some(&dest), &["cat-file", "-e", missing.trim()])?;
    let output = f.0.join("positive-control.bundle");
    run(
        Some(&dest),
        &["bundle", "create", output.to_str().ok_or("output")?, "HEAD"],
    )?;
    run(
        Some(&dest),
        &["bundle", "verify", output.to_str().ok_or("output")?],
    )?;
    assert_eq!(std::fs::read(&marker).map_err(|e| e.to_string())?, calls);
    Ok(())
}
