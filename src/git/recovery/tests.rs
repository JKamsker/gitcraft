use super::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
pub(super) struct Fixture(pub(super) PathBuf);
impl Fixture {
    pub(super) fn new() -> Result<Self, String> {
        static ID: AtomicU64 = AtomicU64::new(0);
        let p = std::env::temp_dir().join(format!(
            "gitcraft-content-recovery-{}-{}",
            std::process::id(),
            ID.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&p).map_err(|e| e.to_string())?;
        for args in [
            vec!["init", "-q", "-b", "main"],
            vec!["config", "user.name", "Test"],
            vec!["config", "user.email", "test@example.com"],
            vec!["config", "commit.gpgSign", "false"],
        ] {
            git::run(Some(&p), &args)?;
        }
        Ok(Self(p))
    }
    fn write(&self, path: &str, bytes: &[u8]) -> Result<(), String> {
        std::fs::write(self.0.join(path), bytes).map_err(|e| e.to_string())
    }
    fn commit(&self, message: &str) -> Result<String, String> {
        git::run(Some(&self.0), &["add", "-A"])?;
        git::run(Some(&self.0), &["commit", "-qm", message])?;
        Ok(git::head(&self.0))
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn hit<'a>(results: &'a Results, blob: &str) -> Result<&'a Hit, String> {
    results
        .report
        .hits
        .iter()
        .find(|hit| hit.blob == blob)
        .ok_or_else(|| format!("Missing content hit {blob}: {:?}", results.report))
}
#[test]
fn content_search_finds_reflog_commits_and_staged_only_blobs_without_any_writes()
-> Result<(), String> {
    let f = Fixture::new()?;
    f.write("file", b"base\n")?;
    let base = f.commit("base")?;
    f.write("file", b"remember parse_header from Tuesday\n")?;
    let lost = f.commit("lost")?;
    let committed = git::run(Some(&f.0), &["rev-parse", &format!("{lost}:file")])?
        .trim()
        .to_owned();
    git::run(Some(&f.0), &["reset", "--hard", &base])?;
    f.write("staged", b"parse_header staged but never committed\n")?;
    git::run(Some(&f.0), &["add", "staged"])?;
    let staged = git::run(Some(&f.0), &["rev-parse", ":staged"])?
        .trim()
        .to_owned();
    git::run(Some(&f.0), &["reset", "--hard", &base])?;
    let refs = git::run(Some(&f.0), &["show-ref"])?;
    let log = git::run(Some(&f.0), &["reflog", "--all", "--format=%H %gs"])?;
    let index = std::fs::read(f.0.join(".git/index")).map_err(|e| e.to_string())?;
    let objects = git::run(Some(&f.0), &["count-objects", "-v"])?;
    let results = search(&f.0, "parse_header", &|| true)?;
    assert!(
        hit(&results, &committed)?
            .provenance
            .iter()
            .any(|source| source.commit == lost && source.path == "file")
    );
    assert!(hit(&results, &staged)?.provenance.is_empty());
    assert_eq!(git::run(Some(&f.0), &["show-ref"])?, refs);
    assert_eq!(
        git::run(Some(&f.0), &["reflog", "--all", "--format=%H %gs"])?,
        log
    );
    assert_eq!(git::run(Some(&f.0), &["count-objects", "-v"])?, objects);
    assert_eq!(
        std::fs::read(f.0.join(".git/index")).map_err(|e| e.to_string())?,
        index
    );
    assert_eq!(
        std::fs::read(f.0.join("file")).map_err(|e| e.to_string())?,
        b"base\n"
    );
    git::run(Some(&f.0), &["reflog", "expire", "--expire=now", "--all"])?;
    let expired = search(&f.0, "parse_header", &|| true)?;
    assert!(
        hit(&expired, &committed)?
            .provenance
            .iter()
            .any(|source| source.origin == Origin::Unreachable)
    );
    Ok(())
}
#[test]
fn dropped_stashes_retain_working_index_and_untracked_content_provenance() -> Result<(), String> {
    let f = Fixture::new()?;
    f.write("file", b"base\n")?;
    f.commit("base")?;
    f.write("file", b"remember staged parse_header\n")?;
    git::run(Some(&f.0), &["add", "file"])?;
    f.write("file", b"remember working parse_header\n")?;
    f.write("untracked", b"remember untracked parse_header\n")?;
    git::run(
        Some(&f.0),
        &[
            "stash",
            "push",
            "--include-untracked",
            "-qm",
            "remember this",
        ],
    )?;
    let stash = git::run(Some(&f.0), &["rev-parse", "refs/stash"])?
        .trim()
        .to_owned();
    git::run(Some(&f.0), &["stash", "drop"])?;
    let results = search(&f.0, "parse_header", &|| true)?;
    for layer in ["working", "staged", "untracked"] {
        assert!(
            results
                .report
                .hits
                .iter()
                .flat_map(|hit| &hit.provenance)
                .any(|source| source.commit == stash
                    && source.origin == Origin::DroppedStash
                    && source.layer == layer),
            "Missing {layer}: {:?}",
            results.report
        );
    }
    Ok(())
}
#[test]
fn binary_large_unicode_literal_and_cancelled_scans_have_explicit_limits() -> Result<(), String> {
    let f = Fixture::new()?;
    f.write("file", b"base")?;
    f.commit("base")?;
    for (name, bytes) in [
        ("binary", b"parse_header\0secret".to_vec()),
        ("large", vec![b'x'; BLOB_LIMIT + 1]),
        (
            "unicode",
            "🌱 parse_header\nwithout newline".as_bytes().to_vec(),
        ),
    ] {
        f.write(name, &bytes)?;
    }
    f.commit("objects")?;
    let results = search(&f.0, "parse_header", &|| true)?;
    assert_eq!(results.report.binary, 1);
    assert_eq!(results.report.large, 1);
    assert!(
        results
            .report
            .hits
            .iter()
            .any(|hit| hit.excerpt.contains("🌱 parse_header"))
    );
    assert!(search(&f.0, ".*", &|| true)?.report.hits.is_empty());
    assert!(search(&f.0, "parse_header", &|| false).is_err());
    assert!(search(&f.0, " ", &|| true).is_err());
    Ok(())
}
#[test]
fn commit_and_excerpt_parsers_refuse_invalid_identity_and_keep_unicode_ranges() {
    assert!(!valid_oid("--all"));
    let oid = "1".repeat(40);
    let bytes =
        format!("tree {oid}\nauthor T <t@t> 1 +0000\ncommitter T <t@t> 2 +0000\n\nsubject\n");
    assert_eq!(parse_commit(&oid, bytes.as_bytes()).map(|c| c.when), Ok(2));
    assert!(parse_commit(&oid, b"tree --bad\n\nmessage").is_err());
    let text = "x\n🌱 remember this\n";
    let (line, excerpt) = super::excerpt(text, text.find("remember").unwrap_or(0));
    assert_eq!(line, 2);
    assert!(excerpt.contains("🌱 remember"));
}

#[test]
fn prepared_branch_and_file_recovery_are_create_only_and_preserve_head_and_index()
-> Result<(), String> {
    let f = Fixture::new()?;
    f.write("file", b"recover parse_header exactly\n")?;
    let commit = f.commit("source")?;
    let results = search(&f.0, "parse_header", &|| true)?;
    let found = results.report.hits.first().ok_or("No hit")?;
    let preview = inspect(&f.0, &results, found, Some(0), &|| true)?;
    let index = std::fs::read(f.0.join(".git/index")).map_err(|e| e.to_string())?;
    let branch = prepare(
        &f.0,
        &preview,
        Target::Branch("content-rescue".into()),
        &|| true,
    )?;
    perform(&f.0, &branch, &|| true)?;
    assert_eq!(git::resolve(&f.0, "refs/heads/content-rescue")?, commit);
    assert!(perform(&f.0, &branch, &|| true).is_err());
    assert_eq!(git::head(&f.0), commit);
    let file = prepare(
        &f.0,
        &preview,
        Target::File("recovered ü.txt".into()),
        &|| true,
    )?;
    perform(&f.0, &file, &|| true)?;
    assert_eq!(
        std::fs::read(f.0.join("recovered ü.txt")).map_err(|e| e.to_string())?,
        preview.bytes.as_slice()
    );
    assert!(perform(&f.0, &file, &|| true).is_err());
    assert!(prepare(&f.0, &preview, Target::File("file".into()), &|| true).is_err());
    f.write(".gitignore", b"*.secret\n")?;
    assert!(
        prepare(
            &f.0,
            &preview,
            Target::File("private.secret".into()),
            &|| true
        )
        .is_err()
    );
    let race = prepare(&f.0, &preview, Target::File("raced".into()), &|| true)?;
    f.write("raced", b"preserve raced bytes")?;
    assert!(perform(&f.0, &race, &|| true).is_err());
    assert_eq!(
        std::fs::read(f.0.join("raced")).map_err(|e| e.to_string())?,
        b"preserve raced bytes"
    );
    assert_eq!(
        std::fs::read(f.0.join(".git/index")).map_err(|e| e.to_string())?,
        index
    );
    Ok(())
}
#[test]
fn dropped_stash_recovery_only_updates_its_list_and_refuses_a_moved_destination()
-> Result<(), String> {
    let f = Fixture::new()?;
    f.write("file", b"base")?;
    f.commit("base")?;
    f.write("file", b"parse_header working stash")?;
    git::run(Some(&f.0), &["stash", "push", "-qm", "lost"])?;
    let stash = git::resolve(&f.0, "refs/stash")?;
    git::run(Some(&f.0), &["stash", "drop"])?;
    let results = search(&f.0, "parse_header", &|| true)?;
    let found = results
        .report
        .hits
        .iter()
        .find(|hit| {
            hit.provenance
                .iter()
                .any(|source| source.origin == Origin::DroppedStash)
        })
        .ok_or("No stash content")?;
    let selection = found
        .provenance
        .iter()
        .position(|source| source.origin == Origin::DroppedStash)
        .ok_or("No provenance")?;
    let preview = inspect(&f.0, &results, found, Some(selection), &|| true)?;
    let prepared = prepare(&f.0, &preview, Target::Stash, &|| true)?;
    f.write("file", b"different current stash")?;
    git::run(Some(&f.0), &["stash", "push", "-qm", "current"])?;
    let current = git::resolve(&f.0, "refs/stash")?;
    assert!(perform(&f.0, &prepared, &|| true).is_err());
    assert_eq!(git::resolve(&f.0, "refs/stash")?, current);
    let index = std::fs::read(f.0.join(".git/index")).map_err(|e| e.to_string())?;
    let file = std::fs::read(f.0.join("file")).map_err(|e| e.to_string())?;
    let prepared = prepare(&f.0, &preview, Target::Stash, &|| true)?;
    perform(&f.0, &prepared, &|| true)?;
    assert_eq!(git::resolve(&f.0, "refs/stash")?, stash);
    assert_eq!(git::resolve(&f.0, "stash@{1}")?, current);
    assert!(prepare(&f.0, &preview, Target::Stash, &|| true).is_err());
    assert_eq!(
        std::fs::read(f.0.join(".git/index")).map_err(|e| e.to_string())?,
        index
    );
    assert_eq!(
        std::fs::read(f.0.join("file")).map_err(|e| e.to_string())?,
        file
    );
    Ok(())
}
#[cfg(unix)]
#[test]
fn new_file_recovery_refuses_links_special_destinations_and_missing_tracked_paths()
-> Result<(), String> {
    let f = Fixture::new()?;
    f.write("file", b"parse_header saved")?;
    f.commit("base")?;
    let results = search(&f.0, "parse_header", &|| true)?;
    let found = results.report.hits.first().ok_or("No hit")?;
    let preview = inspect(&f.0, &results, found, Some(0), &|| true)?;
    std::fs::remove_file(f.0.join("file")).map_err(|e| e.to_string())?;
    assert!(prepare(&f.0, &preview, Target::File("file".into()), &|| true).is_err());
    std::os::unix::fs::symlink(".git", f.0.join("linked")).map_err(|e| e.to_string())?;
    assert!(
        prepare(
            &f.0,
            &preview,
            Target::File("linked/config-copy".into()),
            &|| true
        )
        .is_err()
    );
    std::fs::create_dir(f.0.join("directory")).map_err(|e| e.to_string())?;
    let prepared = prepare(
        &f.0,
        &preview,
        Target::File("directory/new".into()),
        &|| true,
    )?;
    std::fs::remove_dir(f.0.join("directory")).map_err(|e| e.to_string())?;
    std::os::unix::fs::symlink(".git", f.0.join("directory")).map_err(|e| e.to_string())?;
    assert!(perform(&f.0, &prepared, &|| true).is_err());
    assert!(!f.0.join(".git/new").exists());
    assert!(prepare(&f.0, &preview, Target::File("safe".into()), &|| false).is_err());
    assert!(!f.0.join("safe").exists());
    Ok(())
}
#[test]
fn preview_rejects_forged_provenance_and_pruned_blob_identity() -> Result<(), String> {
    let f = Fixture::new()?;
    f.write("file", b"parse_header saved")?;
    f.commit("base")?;
    let results = search(&f.0, "parse_header", &|| true)?;
    let mut found = results.report.hits.first().ok_or("No hit")?.clone();
    found.provenance[0].path = "wrong".into();
    assert!(inspect(&f.0, &results, &found, Some(0), &|| true).is_err());
    let preview = inspect(&f.0, &results, &results.report.hits[0], Some(0), &|| true)?;
    let mut forged = preview.clone();
    forged.blob = "0".repeat(40);
    assert!(prepare(&f.0, &forged, Target::File("untrusted".into()), &|| true).is_err());
    assert!(!f.0.join("untrusted").exists());
    Ok(())
}
