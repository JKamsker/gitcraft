#[test]
fn absolute_attribution_dependencies_keep_repository_routing_and_follow_file_reads()
-> Result<(), String> {
    let repo = std::path::Path::new("ssh://recorded/home/repo");
    let value =
        attribution_file_input_with(repo, "/home/user/ignore'file", |actual_repo, script| {
            assert_eq!(
                actual_repo, repo,
                "the remote repository is never replaced by an absolute local path"
            );
            assert!(script.contains(&crate::remote::sh_quote("/home/user/ignore'file")));
            assert!(script.contains("git hash-object --stdin <"));
            assert!(
                !script.contains("readlink"),
                "Git follows configured file symlinks"
            );
            Ok(git::ShellRun {
                code: 0,
                stdout: b"file:fingerprint".to_vec(),
                stderr: Vec::new(),
            })
        })?;
    assert_eq!(value, b"file:fingerprint");
    Ok(())
}

#[cfg(unix)]
#[test]
fn blame_key_follows_configured_symlink_target_contents() -> Result<(), String> {
    use std::os::unix::fs::symlink;
    let dir =
        crate::repo::revert_tests::scratch("blame-key-symlink", &[("same.txt", "fixture\n")])?;
    let target = dir.join("ignored-target");
    std::fs::write(&target, b"# first\n").map_err(|e| e.to_string())?;
    symlink(&target, dir.join("ignore-link")).map_err(|e| e.to_string())?;
    git::run(
        Some(&dir),
        &["config", "blame.ignoreRevsFile", "ignore-link"],
    )?;
    let rev = git::resolve(&dir, "HEAD")?;
    let before = blame_input(&dir, Some(&rev), "same.txt").ok_or("missing initial key")?;
    std::fs::write(&target, b"# changed target, unchanged link\n").map_err(|e| e.to_string())?;
    assert_ne!(
        blame_input(&dir, Some(&rev), "same.txt").ok_or("missing changed key")?,
        before
    );
    std::fs::remove_dir_all(dir).map_err(|e| e.to_string())?;
    Ok(())
}

#[test]
fn blame_key_observes_effective_index_attributes_and_moving_mailmap_blob() -> Result<(), String> {
    let dir = crate::repo::revert_tests::scratch(
        "blame-key-attributes-mailmap",
        &[("same.txt", "fixture\n")],
    )?;
    let viewed = git::resolve(&dir, "HEAD")?;
    for driver in ["first", "second"] {
        std::fs::write(
            dir.join(".gitattributes"),
            format!("same.txt diff={driver}\n"),
        )
        .map_err(|e| e.to_string())?;
        git::run(Some(&dir), &["add", ".gitattributes"])?;
        std::fs::remove_file(dir.join(".gitattributes")).map_err(|e| e.to_string())?;
        let attributes = git::run(Some(&dir), &["check-attr", "-z", "--all", "--", "same.txt"])?;
        assert!(attributes.contains(driver), "Git must use index fallback");
        let key = blame_input(&dir, Some(&viewed), "same.txt").ok_or("missing attribute key")?;
        if driver == "first" {
            std::fs::write(dir.join("first-key"), key).map_err(|e| e.to_string())?;
        } else {
            assert_ne!(
                key,
                std::fs::read_to_string(dir.join("first-key")).map_err(|e| e.to_string())?
            );
        }
    }
    git::run(Some(&dir), &["reset", "--hard", "HEAD"])?;
    git::run(Some(&dir), &["config", "mailmap.blob", "HEAD:.mailmap"])?;
    let email = git::run(Some(&dir), &["show", "-s", "--format=%ae", "HEAD"])?;
    let mut before = None;
    for (name, email_to) in [
        ("First", "first@example.invalid"),
        ("Second", "second@example.invalid"),
    ] {
        std::fs::write(
            dir.join(".mailmap"),
            format!("{name} <{email_to}> <{}>\n", email.trim()),
        )
        .map_err(|e| e.to_string())?;
        git::run(Some(&dir), &["add", ".mailmap"])?;
        git::run(Some(&dir), &["commit", "-qm", name])?;
        std::fs::remove_file(dir.join(".mailmap")).map_err(|e| e.to_string())?;
        let key = blame_input(&dir, Some(&viewed), "same.txt").ok_or("missing mailmap key")?;
        let attribution = git::blame(&dir, Some(&viewed), "same.txt")?;
        assert_eq!(
            attribution.first().ok_or("missing blame line")?.email,
            email_to
        );
        if let Some(before) = before {
            assert_ne!(key, before);
        }
        before = Some(key);
    }
    std::fs::remove_dir_all(dir).map_err(|e| e.to_string())?;
    Ok(())
}
