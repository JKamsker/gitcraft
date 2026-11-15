use super::*;
use crate::git::testlane::{Scratch, fixture::Repo};
#[test]
fn late_filter_and_encoding_changes_cannot_affect_raw_materialization() -> Result<(), String> {
    let repo = Repo::new("staged-raw-race")?;
    repo.commit("base", &[("payload", Some("a\nb\n"))])?;
    crate::git::command::snapshot_policy::without_hooks(|| {
        let tree = repo.git(&["write-tree"])?;
        let scratch = Scratch::add_empty(repo.path(), "test", "HEAD")?;
        let entries = manifest(repo.path(), tree.trim(), &scratch.path)?;
        // Deterministically insert the review's race after attribute refusal, before
        // any materialization. Both process and smudge drivers would write markers.
        for (driver, marker) in [("smudge", "smudge-ran"), ("process", "process-ran")] {
            let path = repo
                .path()
                .join(marker)
                .to_string_lossy()
                .replace('\\', "/");
            let path = path.strip_prefix("//?/").unwrap_or(&path);
            repo.git(&[
                "config",
                &format!("filter.probe.{driver}"),
                &format!("echo unsafe > {}; exit 1", crate::remote::sh_quote(path)),
            ])?;
        }
        repo.git(&["config", "filter.probe.required", "true"])?;
        repo.git(&["config", "core.autocrlf", "true"])?;
        std::fs::write(
            repo.path().join(".git/info/attributes"),
            "payload filter=probe text eol=crlf working-tree-encoding=UTF-16LE\n",
        )
        .map_err(|e| e.to_string())?;
        materialize(repo.path(), &scratch.path, &entries)?;
        faithful(&scratch.path, &entries)?;
        assert_eq!(
            std::fs::read(scratch.path.join("payload")).map_err(|e| e.to_string())?,
            b"a\nb\n"
        );
        assert!(!repo.path().join("smudge-ran").exists());
        assert!(!repo.path().join("process-ran").exists());
        scratch.remove()?;
        Ok(())
    })
}
#[test]
fn staged_crlf_and_encoding_attributes_do_not_expand_indexed_bytes() -> Result<(), String> {
    let repo = Repo::new("staged-raw-encoding")?;
    repo.commit("base", &[("payload", Some("a\nb\n"))])?;
    std::fs::write(
        repo.path().join(".gitattributes"),
        "payload text eol=crlf working-tree-encoding=UTF-16LE\n",
    )
    .map_err(|e| e.to_string())?;
    // Add only the attributes. The payload's indexed UTF-8 bytes are the contract;
    // a normal checkout would double them through encoding/line ending conversion.
    repo.git(&[
        "-c",
        "core.checkRoundtripEncoding=",
        "add",
        ".gitattributes",
    ])?;
    let (scratch, _) = super::super::snapshot(repo.path(), None)?;
    assert_eq!(
        std::fs::read(scratch.path.join("payload")).map_err(|e| e.to_string())?,
        b"a\nb\n"
    );
    scratch.remove()?;
    Ok(())
}

#[test]
fn raw_blob_identity_is_verified_for_sha1_and_sha256_before_writing() -> Result<(), String> {
    use std::io::Write;
    for format in ["sha1", "sha256"] {
        let repo = tempfile::tempdir().map_err(|e| e.to_string())?;
        git::run(
            Some(repo.path()),
            &["init", "-q", &format!("--object-format={format}")],
        )?;
        let stored = git::run_captured(
            repo.path(),
            &["hash-object", "-w", "--stdin"],
            Some(b"good"),
        )?;
        assert_eq!(stored.code, Some(0));
        let oid = stored.stdout.trim().to_owned();
        assert_eq!(oid.len(), if format == "sha1" { 40 } else { 64 });
        let entries = [Entry {
            path: "payload".into(),
            mode: "100644".into(),
            oid: oid.clone(),
            size: 4,
        }];
        let valid = tempfile::tempdir().map_err(|e| e.to_string())?;
        materialize(repo.path(), valid.path(), &entries)?;
        assert_eq!(
            std::fs::read(valid.path().join("payload")).map_err(|e| e.to_string())?,
            b"good"
        );
        // A valid zlib stream with the same type/length, stored under the old OID.
        // cat-file accepts this corruption; materialization must recompute identity.
        let mut compressed =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        compressed
            .write_all(b"blob 4\0evil")
            .map_err(|e| e.to_string())?;
        let compressed = compressed.finish().map_err(|e| e.to_string())?;
        std::fs::write(
            repo.path()
                .join(".git/objects")
                .join(&oid[..2])
                .join(&oid[2..]),
            compressed,
        )
        .map_err(|e| e.to_string())?;
        let refused = tempfile::tempdir().map_err(|e| e.to_string())?;
        assert_eq!(
            materialize(repo.path(), refused.path(), &entries),
            Err(fail("staged-object"))
        );
        assert!(!refused.path().join("payload").exists());
    }
    Ok(())
}
