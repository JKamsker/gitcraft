use super::*;
fn entry(path: &str, size: u64) -> Entry {
    Entry {
        path: path.into(),
        mode: "100644".into(),
        oid: String::new(),
        size,
    }
}
#[test]
fn portable_names_are_checked_independently_of_host_filesystem() {
    let root = Path::new("snapshot");
    for pair in [
        ["Caller", "caller"],
        ["Dir/a", "dir/b"],
        ["é", "e\u{301}"],
        ["Σ", "ς"],
    ] {
        assert!(
            portable(&pair.map(|p| entry(p, 0)), root, STORAGE).is_err(),
            "{pair:?}"
        );
    }
    for name in [
        "CON",
        "CON .txt",
        "CONIN$",
        "CONOUT$.txt",
        "aux.txt",
        "LPT1",
        "COM¹.log",
        "end.",
        "trail ",
        "a:b",
        "a\\b",
        "../escape",
        ".GiT/config",
        "GIT~1/config",
        "directory/git~1/HEAD",
        ".g\u{200c}it/config",
        ".git\u{feff}/config",
        ".git::$INDEX_ALLOCATION",
        "GIT~1:stream",
    ] {
        assert!(
            portable(&[entry(name, 0)], root, STORAGE).is_err(),
            "{name}"
        );
    }
    assert!(portable(&[entry(&"a".repeat(241), 0)], root, STORAGE).is_err());
    assert!(portable(&[entry("ordinary", 0)], root, STORAGE).is_ok());
}
#[test]
fn storage_includes_block_rounding_and_namespace_overhead() {
    assert!(portable(&[entry("large", STORAGE)], Path::new("s"), STORAGE).is_err());
    assert!(portable(&[entry("a/b", 1)], Path::new("s"), 8192).is_err());
    assert!(portable(&[entry("a/b", 1)], Path::new("s"), 12288).is_ok());
}

#[test]
fn actual_directory_entries_must_match_the_requested_spelling_and_membership() -> Result<(), String>
{
    let root = tempfile::tempdir().map_err(|e| e.to_string())?;
    std::fs::write(root.path().join("Canonical"), "x").map_err(|e| e.to_string())?;
    assert!(namespace(root.path(), &[entry("canonical", 1)]).is_err());
    assert!(namespace(root.path(), &[entry("Canonical", 1)]).is_ok());
    std::fs::write(root.path().join("unexpected"), "x").map_err(|e| e.to_string())?;
    assert!(namespace(root.path(), &[entry("Canonical", 1)]).is_err());
    Ok(())
}

#[test]
fn every_staged_symlink_is_refused_without_resolving_its_target() {
    for target_size in [0, 1, 1024] {
        let mut link = entry("ordinary", target_size);
        link.mode = "120000".into();
        assert_eq!(
            portable(&[link], Path::new("snapshot"), STORAGE),
            Err(fail("staged-links"))
        );
    }
}
