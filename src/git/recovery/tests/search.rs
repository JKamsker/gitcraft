//! Search.

use super::*;

#[test]
fn shared_blobs_keep_each_path_and_stash_layer_before_and_after_drop() -> Result<(), String> {
    for dropped in [false, true] {
        let f = Fixture::new()?;
        f.write("first", b"parse_header shared history\n")?;
        f.write("second", b"parse_header shared history\n")?;
        let history = f.commit("shared sources")?;
        f.write("first", b"parse_header shared stash layers\n")?;
        git::run(Some(&f.0), &["add", "first"])?;
        git::run(Some(&f.0), &["stash", "push", "-qm", "shared layers"])?;
        let stash = git::resolve(&f.0, "refs/stash")?;
        if dropped {
            git::run(Some(&f.0), &["stash", "drop"])?;
        }
        let results = search(&f.0, "parse_header", &|| true)?;
        assert_eq!(results.report.hits.len(), 2);
        for path in ["first", "second"] {
            assert!(
                results
                    .report
                    .hits
                    .iter()
                    .flat_map(|hit| &hit.provenance)
                    .any(|source| {
                        source.commit == history
                            && source.path == path
                            && source.origin == Origin::History
                    }),
                "Missing history path {path}: {:?}",
                results.report
            );
        }
        let expected = if dropped {
            Origin::DroppedStash
        } else {
            Origin::LiveStash
        };
        for layer in ["working", "staged"] {
            assert!(
                results
                    .report
                    .hits
                    .iter()
                    .flat_map(|hit| &hit.provenance)
                    .any(|source| {
                        source.commit == stash && source.layer == layer && source.origin == expected
                    }),
                "Missing {expected:?} layer {layer}: {:?}",
                results.report
            );
        }
    }
    Ok(())
}

#[test]
pub(super) fn content_search_finds_reflog_commits_and_staged_only_blobs_without_any_writes()
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
pub(super) fn dropped_stashes_retain_working_index_and_untracked_content_provenance()
-> Result<(), String> {
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
pub(super) fn binary_large_unicode_literal_and_cancelled_scans_have_explicit_limits()
-> Result<(), String> {
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
pub(super) fn commit_and_excerpt_parsers_refuse_invalid_identity_and_keep_unicode_ranges() {
    assert!(!valid_oid("--all"));
    let oid = "1".repeat(40);
    let bytes =
        format!("tree {oid}\nauthor T <t@t> 1 +0000\ncommitter T <t@t> 2 +0000\n\nsubject\n");
    assert_eq!(parse_commit(&oid, bytes.as_bytes()).map(|c| c.when), Ok(2));
    assert!(parse_commit(&oid, b"tree --bad\n\nmessage").is_err());
    let text = "x\n🌱 remember this\n";
    let (line, excerpt) = super::super::excerpt(text, text.find("remember").unwrap_or(0));
    assert_eq!(line, 2);
    assert!(excerpt.contains("🌱 remember"));
}
