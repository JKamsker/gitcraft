use super::*;
use crate::git::lfs_test_server::lfs_repo;

#[test]
fn verified_locks_are_ours_or_theirs() -> Result<(), String> {
    let json = r#"{"ours":[{"id":"2","path":"b.psd","owner":{"name":"me"},
        "locked_at":"2026-11-01T10:00:00Z"}],
        "theirs":[{"id":"1","path":"art/a b.psd","owner":{"name":"Ana"},
        "locked_at":"2026-11-01T12:30:00+02:00"}]}"#;
    let locks = parse_verified(json)?;
    assert_eq!(locks.len(), 2);
    assert_eq!(locks[0].path, "art/a b.psd");
    assert_eq!(locks[0].owner, "Ana");
    assert!(locks[0].theirs());
    assert_eq!(locks[1].ours, Some(true));
    // 12:30 at +02:00 is 10:30 UTC: half an hour after the other.
    assert_eq!(
        locks[0]
            .locked_at
            .zip(locks[1].locked_at)
            .map(|(a, b)| a - b),
        Some(1800)
    );
    Ok(())
}

#[test]
fn listed_locks_are_nobodys_for_sure() -> Result<(), String> {
    assert!(parse_listed("")?.is_empty());
    assert!(parse_listed("[]")?.is_empty());
    let locks = parse_listed(r#"[{"id":"7","path":"ü\\x.png","owner":{"name":"Zoë"}}]"#)?;
    assert_eq!(locks[0].path, "ü/x.png");
    assert_eq!(locks[0].ours, None);
    assert_eq!(locks[0].locked_at, None);
    assert!(!locks[0].theirs());
    assert!(parse_listed("not json").is_err());
    Ok(())
}

#[test]
fn rfc3339_times() {
    assert_eq!(parse_rfc3339("1970-01-01T00:00:00Z"), Some(0));
    assert_eq!(parse_rfc3339("2000-03-01T00:00:00Z"), Some(951_868_800));
    assert_eq!(
        parse_rfc3339("2026-11-01T10:00:00.123456Z"),
        parse_rfc3339("2026-11-01T10:00:00Z")
    );
    assert_eq!(parse_rfc3339("1970-01-01T05:30:00+05:30"), Some(0));
    assert_eq!(parse_rfc3339("1970-01-01T00:00:00-0100"), Some(3600));
    for bad in [
        "",
        "yesterday",
        "2026-13-01T00:00:00Z",
        "2026-11-01T25:00:00Z",
        "2026-11-01",
    ] {
        assert_eq!(parse_rfc3339(bad), None, "{bad}");
    }
}

#[test]
fn how_long_a_lock_is_held() {
    assert_eq!(held_for(100, 100), "now");
    assert_eq!(held_for(100, 50), "now");
    assert_eq!(held_for(0, 125), "2 min");
    assert_eq!(held_for(0, 3 * 3600), "3 h");
    assert_eq!(held_for(0, 2 * 86_400 + 5), "2 d");
    assert_eq!(held_for(0, 3 * 31_557_600), "3 y");
}

#[test]
fn lockable_patterns_in_attributes() {
    let text = "# art\n*.psd filter=lfs diff=lfs merge=lfs -text lockable\n\
                *.txt text\n\"my file.bin\" lockable=true\n# *.fbx lockable\n\n";
    assert_eq!(lockable_in(text), ["*.psd", "\"my file.bin\""]);
    assert!(lockable_in("").is_empty());
}

#[test]
fn check_attr_output() {
    let out = "a.psd\0lockable\0set\0b.txt\0lockable\0unspecified\0c d.psd\0lockable\0set\0";
    assert_eq!(parse_check_attr(out), ["a.psd", "c d.psd"]);
    assert!(parse_check_attr("").is_empty());
}

#[test]
fn git_lfs_errors_read_as_one_line() {
    assert_eq!(
        lfs_error("Locking a.psd failed\nError: Lock exists\n"),
        "Lock exists"
    );
    assert_eq!(lfs_error(""), "git-lfs failed without saying why");
}

/// Locks are taken, listed as ours or theirs, given back, and forced off someone else, all
/// through git-lfs against a real (test) server; names with spaces and letters outside ASCII.
#[test]
fn locks_through_git_lfs() -> Result<(), String> {
    let Some((dir, server)) = lfs_repo("locks")? else {
        return Ok(());
    };
    track_lockable(&dir, "*.psd")?;
    for name in ["a b.psd", "ü.psd"] {
        std::fs::write(dir.join(name), "pixels").map_err(|e| e.to_string())?;
    }
    crate::git::run(Some(&dir), &["add", "-A"])?;
    crate::git::run(Some(&dir), &["commit", "-qm", "art"])?;
    assert_eq!(lockable_patterns(&dir), ["*.psd"]);
    let lockable = lockable_paths(
        &dir,
        &[
            "a b.psd".to_owned(),
            "ü.psd".to_owned(),
            ".gitattributes".to_owned(),
        ],
    )?;
    assert_eq!(lockable, ["a b.psd", "ü.psd"]);

    let mine = lfs_lock(&dir, "a b.psd", None)?;
    assert_eq!((mine.path.as_str(), mine.ours), ("a b.psd", Some(true)));
    server.lock_for_someone_else("ü.psd");
    let held = lfs_lock(&dir, "ü.psd", None);
    assert!(
        held.as_ref().is_err_and(|e| e.contains("exists")),
        "{held:?}"
    );

    let locks = lfs_locks(&dir, None, Asked::Background)?;
    let summary: Vec<(&str, Option<bool>, &str)> = locks
        .iter()
        .map(|l| (l.path.as_str(), l.ours, l.owner.as_str()))
        .collect();
    assert_eq!(
        summary,
        [("a b.psd", Some(true), "me"), ("ü.psd", Some(false), "ana")]
    );
    let theirs = locks.iter().find(|l| l.theirs()).cloned();
    let theirs = theirs.ok_or("no lock of someone else")?;
    // Not ours: refused without force, taken with it.
    let pair = |l: &LfsLock| (l.id.clone(), l.path.clone());
    assert!(lfs_unlock(&dir, &[pair(&theirs)], false, None).is_err());
    lfs_unlock(&dir, &[pair(&mine), pair(&theirs)], true, None)?;
    assert!(server.paths().is_empty());
    assert!(lfs_locks(&dir, None, Asked::ByUser)?.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

/// A server that can't verify locks still lists them, nobody's known as ours.
#[test]
fn locks_listed_when_they_cannot_be_verified() -> Result<(), String> {
    let Some((dir, server)) = lfs_repo("no-verify")? else {
        return Ok(());
    };
    server.refuse_verify();
    server.lock_for_someone_else("x.bin");
    let locks = lfs_locks(&dir, None, Asked::Background)?;
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(locks.len(), 1);
    assert_eq!(locks[0].ours, None);
    Ok(())
}

/// Unlocking several where one fails names the one that stayed, and still gives back the rest.
#[test]
fn unlocking_several_names_what_stayed() -> Result<(), String> {
    let Some((dir, server)) = lfs_repo("unlock-some")? else {
        return Ok(());
    };
    let a = lfs_lock(&dir, "a.bin", None)?;
    let gone = ("L999".to_owned(), "gone.bin".to_owned());
    let result = lfs_unlock(&dir, &[gone, (a.id, a.path)], false, None);
    let _ = std::fs::remove_dir_all(&dir);
    let e = result.err().unwrap_or_default();
    assert!(e.starts_with("gone.bin: "), "{e}");
    assert!(server.paths().is_empty());
    Ok(())
}

#[test]
fn an_empty_lockable_pattern_is_refused() {
    let dir = std::env::temp_dir();
    assert!(track_lockable(&dir, "  ").is_err());
}
