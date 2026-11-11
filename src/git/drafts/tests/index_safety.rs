//! Index safety.

use super::*;

#[cfg(unix)]
#[test]
pub(super) fn private_index_preserves_racy_content_checks_across_timestamp_boundaries()
-> Result<(), String> {
    let f = Fixture::new()?;
    f.write("file", b"old\n")?;
    let file_time = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000);
    let set_time = |path: &std::path::Path, time| -> Result<(), String> {
        std::fs::File::options()
            .write(true)
            .open(path)
            .and_then(|f| f.set_times(std::fs::FileTimes::new().set_modified(time)))
            .map_err(|e| e.to_string())
    };
    set_time(&f.0.join("file"), file_time)?;
    f.commit()?;
    let index_path = f.0.join(".git/index");
    let index_time = file_time - std::time::Duration::from_secs(1);
    set_time(&index_path, index_time)?;
    let original_index = std::fs::read(&index_path).map_err(|e| e.to_string())?;
    f.write("file", b"new\n")?;
    set_time(&f.0.join("file"), file_time)?;
    let ctx = Context::new(&f.0, &|| true)?;
    // Model a filesystem whose stat tick cannot distinguish these two writes.
    // The source remains byte-for-byte unchanged; only this private context uses
    // minimal stat comparisons so the racy content check is essential.
    for (key, value) in [("core.trustctime", "false"), ("core.checkStat", "minimal")] {
        ctx.run(&["config", key, value], vec![], &[0], &|| true)?;
    }
    let before = source(&ctx, &|| true)?;
    let index_tree = tree(&ctx, &before.index, &|| true)?;
    for _ in 0..2 {
        cached_index(&ctx, &before, &|| true)?;
        assert!(changed(&ctx, &before, &index_tree, &|| true)?.contains("file"));
    }
    assert_eq!(
        std::fs::read(&index_path).map_err(|e| e.to_string())?,
        original_index
    );
    assert_eq!(
        std::fs::metadata(index_path)
            .and_then(|m| m.modified())
            .map_err(|e| e.to_string())?,
        index_time
    );
    assert_eq!(
        std::fs::read(f.0.join("file")).map_err(|e| e.to_string())?,
        b"new\n"
    );
    Ok(())
}

#[cfg(unix)]
#[test]
pub(super) fn configured_filters_hooks_fsmonitor_and_signing_never_run_for_drafts_or_recovery()
-> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new()?;
    f.write("file", b"base\n")?;
    f.write(
        ".gitattributes",
        b"file filter=trap diff=trap merge=trap text eol=crlf\n",
    )?;
    f.commit()?;
    let marker = f.0.join(".git/executed");
    let script = f.0.join(".git/trap");
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\nprintf ran >> {}\nexit 1\n",
            remote::sh_quote(marker.to_str().ok_or("path")?)
        ),
    )
    .map_err(|e| e.to_string())?;
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700))
        .map_err(|e| e.to_string())?;
    for key in [
        "filter.trap.clean",
        "filter.trap.smudge",
        "filter.trap.process",
        "diff.trap.command",
        "diff.trap.textconv",
        "merge.trap.driver",
        "core.fsmonitor",
        "gpg.program",
        "core.sshCommand",
    ] {
        git::run(Some(&f.0), &["config", key, script.to_str().ok_or("path")?])?;
    }
    git::run(Some(&f.0), &["config", "filter.trap.required", "true"])?;
    git::run(Some(&f.0), &["config", "commit.gpgSign", "true"])?;
    // The authorized isolated test environment deliberately has an empty Git
    // template directory. Install this fixture's hook explicitly.
    std::fs::create_dir_all(f.0.join(".git/hooks")).map_err(|e| e.to_string())?;
    std::fs::copy(&script, f.0.join(".git/hooks/reference-transaction"))
        .map_err(|e| e.to_string())?;
    f.write("file", b"saved\r\n")?;
    let index = std::fs::read(f.0.join(".git/index")).map_err(|e| e.to_string())?;
    let draft = capture(&f.0, "Pause", &|| true)?.ok_or("no draft")?;
    f.write("file", b"new\r\n")?;
    let view = preview(&f.0, &draft.id, "file", false, &|| true)?;
    restore(&f.0, &view, &[0], &|| true)?;
    assert_eq!(
        std::fs::read(f.0.join("file")).map_err(|e| e.to_string())?,
        b"saved\r\n"
    );
    assert!(!marker.exists());
    assert_eq!(
        std::fs::read(f.0.join(".git/index")).map_err(|e| e.to_string())?,
        index
    );
    // Verify the hostile source configuration is live, rather than a vacuous test setup.
    let _ = git::run(Some(&f.0), &["status", "--porcelain"]);
    assert!(marker.exists());
    Ok(())
}
