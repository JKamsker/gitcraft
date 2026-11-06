//! The extras of a commit's details on real repositories: its signature, where it is
//! contained, its notes and the later commits linking to it.

use std::path::{Path, PathBuf};

use super::*;

/// A fresh repository named after `name`, with nothing of the user's config that changes what
/// is tested (signing by default, another signing program).
fn repo(name: &str) -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join(format!("gitcraft-extras-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "Zoë Tester"],
        &["config", "user.email", "t@example.org"],
        &["config", "commit.gpgSign", "false"],
        &["config", "tag.gpgSign", "false"],
        &["config", "gpg.ssh.program", "ssh-keygen"],
        &["config", "core.autocrlf", "false"],
    ] {
        run(Some(&dir), args)?;
    }
    Ok(dir)
}

fn commit(dir: &Path, message: &str) -> Result<String, String> {
    run(Some(dir), &["commit", "-q", "--allow-empty", "-m", message])?;
    Ok(run(Some(dir), &["rev-parse", "HEAD"])?.trim().to_owned())
}

fn ssh_keygen_available() -> bool {
    std::process::Command::new("ssh-keygen")
        .arg("-?")
        .output()
        .is_ok()
}

/// A throwaway ed25519 key at `path` (and `path.pub`).
fn keygen(path: &Path) -> Result<String, String> {
    let out = std::process::Command::new("ssh-keygen")
        .args(["-q", "-t", "ed25519", "-N", "", "-C", "throwaway", "-f"])
        .arg(path)
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).into_owned());
    }
    std::fs::read_to_string(path.with_extension("pub")).map_err(|e| e.to_string())
}

/// A commit signed with a throwaway SSH key checks as valid against an allowed signers file
/// that has it; as of an unknown key against one that doesn't; as invalid once its message is
/// changed; and as not checked with nothing to check against. An unsigned one is unsigned.
#[cfg(unix)]
#[test]
fn ssh_signatures_valid_tampered_unknown_and_unchecked() -> Result<(), String> {
    if !ssh_keygen_available() {
        eprintln!("ssh-keygen isn't installed: SSH signatures not tested");
        return Ok(());
    }
    let dir = repo("ssh-signature")?;
    let keys = dir.join(".keys");
    std::fs::create_dir_all(&keys).map_err(|e| e.to_string())?;
    let key = keys.join("signer");
    let public = keygen(&key)?;
    let other = keygen(&keys.join("stranger"))?;
    let allowed = keys.join("allowed signers");
    let key_path = key.to_string_lossy().into_owned();
    for args in [
        &["config", "gpg.format", "ssh"][..],
        &["config", "user.signingKey", &key_path],
    ] {
        run(Some(&dir), args)?;
    }
    run(
        Some(&dir),
        &["commit", "-q", "-S", "--allow-empty", "-m", "signed ✓"],
    )?;
    let signed = run(Some(&dir), &["rev-parse", "HEAD"])?.trim().to_owned();
    let unsigned = commit(&dir, "unsigned")?;

    // Nothing to check against yet.
    let sig = verify_signature(&dir, &signed)?;
    assert_eq!(sig.verdict, Verdict::NotChecked, "{sig:?}");
    assert_eq!(sig.format, Some(SignatureFormat::Ssh));
    assert!(sig.raw.contains("allowedSignersFile"), "{}", sig.raw);

    let allowed_path = allowed.to_string_lossy().into_owned();
    run(
        Some(&dir),
        &["config", "gpg.ssh.allowedSignersFile", &allowed_path],
    )?;
    std::fs::write(&allowed, format!("t@example.org {public}")).map_err(|e| e.to_string())?;
    let sig = verify_signature(&dir, &signed)?;
    assert_eq!(sig.verdict, Verdict::Valid { trusted: true }, "{sig:?}");
    assert_eq!(sig.signer, "t@example.org");
    assert!(sig.key.starts_with("SHA256:"), "{}", sig.key);
    assert!(sig.raw.contains("Good"), "{}", sig.raw);

    // Unsigned: told without running anything.
    let sig = verify_signature(&dir, &unsigned)?;
    assert_eq!(sig.verdict, Verdict::Unsigned);
    assert_eq!(sig.format, None);

    // The same signature over a changed message.
    let object = run(Some(&dir), &["cat-file", "commit", &signed])?;
    let changed = object.replace("signed ✓", "signed, then changed");
    let forged = run_input(
        Some(&dir),
        &["hash-object", "-t", "commit", "-w", "--stdin"],
        &changed,
    )?;
    let sig = verify_signature(&dir, forged.trim())?;
    assert_eq!(sig.verdict, Verdict::Invalid, "{sig:?}");

    // Only someone else is allowed.
    std::fs::write(&allowed, format!("x@example.org {other}")).map_err(|e| e.to_string())?;
    let sig = verify_signature(&dir, &signed)?;
    assert_eq!(sig.verdict, Verdict::UnknownKey, "{sig:?}");

    // No program to check with: not "invalid", as git would have it.
    run(
        Some(&dir),
        &["config", "gpg.ssh.program", "/nonexistent/ssh-keygen"],
    )?;
    let sig = verify_signature(&dir, &signed)?;
    assert_eq!(sig.verdict, Verdict::NotChecked, "{sig:?}");
    assert!(!sig.raw.is_empty());

    assert!(verify_signature(&dir, "0000000000000000000000000000000000000000").is_err());
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

/// Notes round trip in two namespaces: added, read, edited and deleted, each namespace apart;
/// a huge note and one with unicode and blank lines kept; pushed to a remote and fetched from
/// it into a clone.
#[test]
fn notes_round_trip_in_two_namespaces() -> Result<(), String> {
    let dir = repo("notes")?;
    let hash = commit(&dir, "noted")?;
    assert_eq!(notes_namespaces(&dir)?, [DEFAULT_NOTES]);
    assert_eq!(read_note(&dir, DEFAULT_NOTES, &hash)?, None);

    write_note(
        &dir,
        DEFAULT_NOTES,
        &hash,
        "Reviewed by Zoë ✓\n\nBuild: #42",
    )?;
    write_note(&dir, "review/ci", &hash, "CI passed")?;
    assert_eq!(
        read_note(&dir, DEFAULT_NOTES, &hash)?.as_deref(),
        Some("Reviewed by Zoë ✓\n\nBuild: #42\n")
    );
    assert_eq!(
        read_note(&dir, "refs/notes/review/ci", &hash)?.as_deref(),
        Some("CI passed\n")
    );
    assert_eq!(notes_namespaces(&dir)?, ["commits", "review/ci"]);

    // Edited in one namespace, the other stays.
    write_note(&dir, "review/ci", &hash, "CI failed, then passed")?;
    assert_eq!(
        read_note(&dir, "review/ci", &hash)?.as_deref(),
        Some("CI failed, then passed\n")
    );
    assert_eq!(
        read_note(&dir, DEFAULT_NOTES, &hash)?.as_deref(),
        Some("Reviewed by Zoë ✓\n\nBuild: #42\n")
    );

    // Larger than any command line would take.
    let huge = "line of a long build log\n".repeat(20_000);
    write_note(&dir, DEFAULT_NOTES, &hash, &huge)?;
    assert_eq!(read_note(&dir, DEFAULT_NOTES, &hash)?, Some(huge));

    // Deleted, and an empty text deletes too; deleting what isn't there is fine.
    remove_note(&dir, DEFAULT_NOTES, &hash)?;
    assert_eq!(read_note(&dir, DEFAULT_NOTES, &hash)?, None);
    remove_note(&dir, DEFAULT_NOTES, &hash)?;
    write_note(&dir, "review/ci", &hash, "  \n ")?;
    assert_eq!(read_note(&dir, "review/ci", &hash)?, None);
    assert!(write_note(&dir, "bad name", &hash, "x").is_err());

    // Pushed to a bare remote, fetched into a clone.
    write_note(&dir, "review/ci", &hash, "shared note")?;
    let bare = dir.with_file_name(format!("{}-remote", file_name(&dir)));
    let clone = dir.with_file_name(format!("{}-clone", file_name(&dir)));
    for d in [&bare, &clone] {
        let _ = std::fs::remove_dir_all(d);
    }
    let bare_s = bare.to_string_lossy().into_owned();
    run(None, &["init", "-q", "--bare", &bare_s])?;
    run(Some(&dir), &["remote", "add", "my-remote", &bare_s])?;
    run(Some(&dir), &["push", "-q", "my-remote", "main"])?;
    let push = push_notes_args("my-remote", "review/ci")?;
    run(
        Some(&dir),
        &push.iter().map(String::as_str).collect::<Vec<_>>(),
    )?;
    run(None, &["clone", "-q", &bare_s, &clone.to_string_lossy()])?;
    assert_eq!(read_note(&clone, "review/ci", &hash)?, None);
    let fetch = fetch_notes_args("origin", "review/ci")?;
    run(
        Some(&clone),
        &fetch.iter().map(String::as_str).collect::<Vec<_>>(),
    )?;
    assert_eq!(
        read_note(&clone, "review/ci", &hash)?.as_deref(),
        Some("shared note\n")
    );
    for d in [&dir, &bare, &clone] {
        let _ = std::fs::remove_dir_all(d);
    }
    Ok(())
}

fn file_name(dir: &Path) -> String {
    dir.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// The branches (local and remote, with spaces-free unicode names) and tags that contain a
/// commit, and the first tag after it; a commit after every tag has none.
#[test]
fn containment_and_the_first_tag_after_a_commit() -> Result<(), String> {
    let dir = repo("contained")?;
    let first = commit(&dir, "first")?;
    let second = commit(&dir, "second")?;
    run(Some(&dir), &["tag", "v1.0"])?;
    run(Some(&dir), &["tag", "-a", "-m", "annotated", "v1.1"])?;
    run(Some(&dir), &["branch", "feature/ünïcode", &first])?;
    let last = commit(&dir, "after the tags")?;

    let found = containment(&dir, &first)?;
    assert_eq!(found.local.names, ["feature/ünïcode", "main"]);
    assert_eq!(found.tags.names, ["v1.0", "v1.1"]);
    assert_eq!(found.describe.as_deref(), Some("v1.0~1"));
    assert_eq!(found.first_tag(), Some("v1.0"));

    let found = containment(&dir, &second)?;
    assert_eq!(found.local.names, ["main"]);
    assert_eq!(found.first_tag(), Some("v1.0"));

    let found = containment(&dir, &last)?;
    assert_eq!(found.local.names, ["main"]);
    assert!(found.tags.names.is_empty());
    assert_eq!(found.describe, None);

    // Remote branches, without the remote's HEAD.
    let clone = dir.with_file_name(format!("{}-clone", file_name(&dir)));
    let _ = std::fs::remove_dir_all(&clone);
    run(
        None,
        &[
            "clone",
            "-q",
            &dir.to_string_lossy(),
            &clone.to_string_lossy(),
        ],
    )?;
    let found = containment(&clone, &first)?;
    assert_eq!(found.local.names, ["main"]);
    assert_eq!(
        found.remote.names,
        ["origin/feature/ünïcode", "origin/main"]
    );

    // Many tags: capped, counted.
    for n in 0..CONTAINED_CAP + 3 {
        run(Some(&dir), &["tag", &format!("bulk-{n:03}"), &first])?;
    }
    let found = containment(&dir, &first)?;
    assert_eq!(found.tags.names.len(), CONTAINED_CAP);
    assert_eq!(found.tags.total, CONTAINED_CAP + 5);
    for d in [&dir, &clone] {
        let _ = std::fs::remove_dir_all(d);
    }
    Ok(())
}

/// Later commits mentioning a commit by its full or short hash: a revert, a cherry-pick on
/// another branch, a `Fixes:` trailer and a bare mention, the explicit links told apart; a
/// lookalike hash, the commit itself and a stash made on it are not backlinks.
#[test]
fn backlinks_with_short_and_full_hashes() -> Result<(), String> {
    let dir = repo("backlinks")?;
    std::fs::write(dir.join("file with spaces.txt"), "one\n").map_err(|e| e.to_string())?;
    run(Some(&dir), &["add", "-A"])?;
    run(Some(&dir), &["commit", "-q", "-m", "the target"])?;
    let target = run(Some(&dir), &["rev-parse", "HEAD"])?.trim().to_owned();
    let short = &target[..7];
    let twelve = &target[..12];

    // A stash made on it names it; it is no backlink.
    std::fs::write(dir.join("file with spaces.txt"), "dirty\n").map_err(|e| e.to_string())?;
    run(Some(&dir), &["stash", "-q"])?;

    let mention = commit(
        &dir,
        &format!("Follow-up\n\nThis builds on {short}, see there."),
    )?;
    let fixes = commit(
        &dir,
        &format!("Fix the thing\n\nFixes: {twelve} (\"the target\")"),
    )?;
    // Seven hex digits that aren't its start, and its start inside a longer word.
    commit(&dir, &format!("Unrelated 0000000 and x{short}y"))?;
    run(Some(&dir), &["revert", "--no-edit", &target])?;
    let revert = run(Some(&dir), &["rev-parse", "HEAD"])?.trim().to_owned();
    run(Some(&dir), &["checkout", "-q", "-b", "other", &target])?;
    run(Some(&dir), &["commit", "-q", "--allow-empty", "-m", "base"])?;
    run(
        Some(&dir),
        &["cherry-pick", "-x", "--allow-empty", &mention],
    )?;
    let picked = run(Some(&dir), &["rev-parse", "HEAD"])?.trim().to_owned();

    let (links, more) = backlinks(&dir, &target)?;
    assert!(!more);
    let found: Vec<(&str, &LinkKind)> = links.iter().map(|l| (&l.hash[..], &l.kind)).collect();
    assert!(
        found.contains(&(&revert[..], &LinkKind::Revert)),
        "{found:?}"
    );
    assert!(
        found.contains(&(&fixes[..], &LinkKind::Trailer("Fixes".to_owned()))),
        "{found:?}"
    );
    assert!(
        found.contains(&(&mention[..], &LinkKind::Mention)),
        "{found:?}"
    );
    // The cherry-pick names the mention it was picked from, and mentions the target too.
    assert!(
        found.contains(&(&picked[..], &LinkKind::Mention)),
        "{found:?}"
    );
    assert_eq!(links.len(), 4, "{found:?}");
    let fix = links.iter().find(|l| l.hash == fixes);
    assert_eq!(
        fix.map(|l| l.text.as_str()),
        Some(&format!("Fixes: {twelve} (\"the target\")")[..])
    );
    assert_eq!(fix.map(|l| l.author.as_str()), Some("Zoë Tester"));

    // From the mention's side: the cherry-pick is explicit.
    let (links, _) = backlinks(&dir, &mention)?;
    assert_eq!(
        links
            .iter()
            .map(|l| (&l.hash[..], &l.kind))
            .collect::<Vec<_>>(),
        [(&picked[..], &LinkKind::CherryPick)]
    );
    // Nothing mentions the newest commit.
    assert_eq!(backlinks(&dir, &picked)?, (Vec::new(), false));
    assert!(backlinks(&dir, "abc").is_err());
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}
