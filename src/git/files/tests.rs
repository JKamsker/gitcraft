use super::*;

/// `1:x` in the index is that file, not stage 1 of `x`; `[ab].txt` isn't tracked because
/// `a.txt` is.
#[test]
fn odd_names_in_the_index() -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!("gitcraft-odd-names-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let git = |args: &[&str]| run(Some(&dir), args);
    git(&["init", "-q"])?;
    std::fs::write(dir.join("a.txt"), "a\n").map_err(|e| e.to_string())?;
    git(&["add", "a.txt"])?;
    // No such file on disk: `:` isn't allowed in Windows file names, and Git for Windows
    // refuses it even in the index.
    let staged = if cfg!(windows) {
        None
    } else {
        let oid = git(&["hash-object", "-w", "a.txt"])?;
        let info = format!("100644,{},1:x", oid.trim());
        git(&["update-index", "--add", "--cacheinfo", &info])?;
        Some(file_at(&dir, Some(""), "1:x"))
    };
    let tracked = (is_tracked(&dir, "[ab].txt"), is_tracked(&dir, "a.txt"));
    let _ = std::fs::remove_dir_all(&dir);
    if let Some(staged) = staged {
        assert_eq!(staged?, b"a\n");
    }
    assert_eq!(tracked, (false, true));
    Ok(())
}

/// A link on disk is read as git stores it, the path it holds, not what it points to.
#[cfg(unix)]
#[test]
fn a_link_on_disk_is_its_target_path() -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!("gitcraft-disk-link-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::os::unix::fs::symlink("/dev/zero", dir.join("x")).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(dir.join("sub")).map_err(|e| e.to_string())?;
    let link = file_at(&dir, None, "x");
    let folder = file_at(&dir, None, "sub");
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(link?, b"/dev/zero");
    assert!(folder.is_err());
    Ok(())
}

/// A `blame.ignoreRevsFile` this repository hasn't got doesn't keep the file from its blame.
#[test]
fn blame_without_the_ignore_revs_file() -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!("gitcraft-blame-revs-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let git = |args: &[&str]| run(Some(&dir), args);
    for args in [
        &["init", "-q"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["config", "commit.gpgSign", "false"],
        &["config", "blame.ignoreRevsFile", ".git-blame-ignore-revs"],
    ] {
        git(args)?;
    }
    std::fs::write(dir.join("a.txt"), "one\n").map_err(|e| e.to_string())?;
    git(&["add", "a.txt"])?;
    git(&["commit", "-qm", "first"])?;
    let lines = blame(&dir, None, "a.txt");
    let _ = std::fs::remove_dir_all(&dir);
    let lines = lines?;
    assert_eq!(lines.len(), 1);
    assert_eq!(
        (lines[0].summary.as_str(), lines[0].text.as_str()),
        ("first", "one")
    );
    Ok(())
}

#[test]
fn gitignore_lines() {
    assert_eq!(gitignore_with("", "/a.log").as_deref(), Some("/a.log\n"));
    assert_eq!(
        gitignore_with("target\n", "*.log").as_deref(),
        Some("target\n*.log\n")
    );
    // No trailing newline, CRLF kept.
    assert_eq!(
        gitignore_with("a\r\nb", "c").as_deref(),
        Some("a\r\nb\r\nc\r\n")
    );
    assert_eq!(gitignore_with("x\n*.log\n", "*.log"), None);
}

#[test]
fn file_history_follows_renames() {
    let out = format!(
        "{RS}aaa{FS}Ann{FS}ann@x{FS}200{FS}Rename\0\nsrc/new \"q\".rs\0{RS}bbb{FS}Bob{FS}bob@x{FS}100{FS}Add\0\nold.rs\0"
    );
    let h = parse_file_history(&out);
    assert_eq!(h.len(), 2);
    assert_eq!(
        (h[0].hash.as_str(), h[0].path.as_str()),
        ("aaa", "src/new \"q\".rs")
    );
    assert_eq!(
        (h[1].author.as_str(), h[1].time, h[1].path.as_str()),
        ("Bob", 100, "old.rs")
    );
}

#[test]
fn blame_reuses_commit_headers() {
    let a = "a".repeat(40);
    let z = "0".repeat(40);
    let out = format!(
        "{a} 1 1 2\nauthor Ann\nauthor-mail <ann@x>\nauthor-time 100\nsummary First\nfilename f\n\tone\n\
         {a} 2 2\n\ttwo\n\
         {z} 3 3 1\nauthor Not Committed Yet\nauthor-mail <not.committed.yet>\nauthor-time 200\nsummary Version of f from f\nfilename f\n\tthree\n"
    );
    let b = parse_blame(&out);
    assert_eq!(b.len(), 3);
    assert_eq!(
        (
            b[0].author.as_str(),
            b[0].email.as_str(),
            b[0].text.as_str()
        ),
        ("Ann", "ann@x", "one")
    );
    assert_eq!(
        (b[1].summary.as_str(), b[1].time, b[1].text.as_str()),
        ("First", 100, "two")
    );
    assert!(!b[1].uncommitted() && b[2].uncommitted());
}

#[test]
fn blame_of_a_sha256_repository() {
    let a = "a".repeat(64);
    let out = format!("{a} 1 1 1\nauthor Ann\nsummary First\nfilename f\n\tone\n");
    let b = parse_blame(&out);
    assert_eq!(b.len(), 1);
    assert_eq!((b[0].hash.len(), b[0].author.as_str()), (64, "Ann"));
}
