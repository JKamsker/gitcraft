use super::*;
use std::path::PathBuf;
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Result<Self, String> {
        Self::new_with_format("sha1")
    }
    fn new_with_format(format: &str) -> Result<Self, String> {
        static ID: AtomicU64 = AtomicU64::new(0);
        let p = std::env::temp_dir().join(format!(
            "gitcraft-draft-test-{}-{}",
            std::process::id(),
            ID.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&p).map_err(|e| e.to_string())?;
        for args in [
            vec!["init", "-q", "--object-format", format],
            vec!["config", "user.name", "Test"],
            vec!["config", "user.email", "test@example.com"],
            vec!["config", "commit.gpgSign", "false"],
            vec!["config", "core.autocrlf", "false"],
        ] {
            git::run(Some(&p), &args)?;
        }
        Ok(Self(p))
    }
    fn write(&self, path: &str, data: &[u8]) -> Result<(), String> {
        let p = self.0.join(path);
        std::fs::create_dir_all(p.parent().ok_or("no parent")?)
            .and_then(|_| std::fs::write(p, data))
            .map_err(|e| e.to_string())
    }
    fn commit(&self) -> Result<(), String> {
        git::run(Some(&self.0), &["add", "-A"])?;
        git::run(Some(&self.0), &["commit", "-qm", "base"]).map(|_| ())
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
#[test]
fn mixed_index_work_and_untracked_are_independent_and_ignored_files_stay_out() -> Result<(), String>
{
    let f = Fixture::new()?;
    f.write("file", b"base\n")?;
    f.write(".gitignore", b"*.secret\n")?;
    f.commit()?;
    f.write("file", b"staged\n")?;
    git::run(Some(&f.0), &["add", "file"])?;
    f.write("file", b"unstaged\n")?;
    f.write("new file", b"untracked\0binary")?;
    f.write("private.secret", b"never copy")?;
    let index = std::fs::read(f.0.join(".git/index")).map_err(|e| e.to_string())?;
    let snapshot = capture(&f.0, "Pause", &|| true)?.ok_or("no snapshot")?;
    assert!(
        snapshot
            .metadata
            .files
            .iter()
            .any(|p| p.path == "new file" && p.untracked)
    );
    assert!(
        !snapshot
            .metadata
            .files
            .iter()
            .any(|p| p.path == "private.secret")
    );
    let got = git::run_bytes(
        Some(&f.0),
        &[
            "cat-file",
            "blob",
            &format!("{}:file", snapshot.metadata.index_tree),
        ],
    )?;
    assert_eq!(got, b"staged\n");
    assert_eq!(
        git::run_bytes(
            Some(&f.0),
            &[
                "cat-file",
                "blob",
                &format!("{}:file", snapshot.metadata.work_tree)
            ]
        )?,
        b"unstaged\n"
    );
    assert_eq!(
        std::fs::read(f.0.join(".git/index")).map_err(|e| e.to_string())?,
        index
    );
    assert_eq!(list(&f.0, &|| true)?[0].id, snapshot.id);
    assert_eq!(
        capture(&f.0, "Pause again", &|| true)?
            .ok_or("no snapshot")?
            .id,
        snapshot.id
    );
    Ok(())
}

#[test]
fn selected_hunks_recover_without_changing_staged_state_and_have_a_safety_version()
-> Result<(), String> {
    let f = Fixture::new()?;
    let base = (0..28).map(|i| format!("line {i}\n")).collect::<String>();
    f.write("file", base.as_bytes())?;
    f.commit()?;
    let saved = base
        .replace("line 1\n", "saved first\n")
        .replace("line 24\n", "saved second\n");
    f.write("file", saved.as_bytes())?;
    let draft = capture(&f.0, "Pause", &|| true)?.ok_or("no draft")?;
    let newer = base
        .replace("line 1\n", "newer first\n")
        .replace("line 24\n", "newer second\n");
    f.write("file", newer.as_bytes())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(f.0.join("file"), std::fs::Permissions::from_mode(0o600))
            .map_err(|e| e.to_string())?;
    }
    let index = std::fs::read(f.0.join(".git/index")).map_err(|e| e.to_string())?;
    let view = preview(&f.0, &draft.id, "file", false, &|| true)?;
    assert_eq!(view.hunks.len(), 2);
    let backup = restore(&f.0, &view, &[0], &|| true)?;
    assert_eq!(
        std::fs::read_to_string(f.0.join("file")).map_err(|e| e.to_string())?,
        newer.replace("newer first\n", "saved first\n")
    );
    assert_eq!(
        std::fs::read(f.0.join(".git/index")).map_err(|e| e.to_string())?,
        index
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(f.0.join("file"))
                .map_err(|e| e.to_string())?
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
    let undo = preview(&f.0, &backup, "file", false, &|| true)?;
    let selected = (0..undo.hunks.len()).collect::<Vec<_>>();
    restore(&f.0, &undo, &selected, &|| true)?;
    assert_eq!(
        std::fs::read(f.0.join("file")).map_err(|e| e.to_string())?,
        newer.as_bytes()
    );
    assert_eq!(
        std::fs::read(f.0.join(".git/index")).map_err(|e| e.to_string())?,
        index
    );
    Ok(())
}

#[test]
fn binary_empty_files_deletions_and_index_layer_recovery_preserve_the_real_index()
-> Result<(), String> {
    let f = Fixture::new()?;
    f.write("file", b"base\n")?;
    f.commit()?;
    f.write("file", b"staged\n")?;
    git::run(Some(&f.0), &["add", "file"])?;
    f.write("file", b"\xff\xfe\x80binary without nul")?;
    f.write("empty", b"")?;
    let draft = capture(&f.0, "Binary", &|| true)?.ok_or("no draft")?;
    f.write("file", b"different\0binary")?;
    std::fs::remove_file(f.0.join("empty")).map_err(|e| e.to_string())?;
    let index = std::fs::read(f.0.join(".git/index")).map_err(|e| e.to_string())?;
    let view = preview(&f.0, &draft.id, "file", false, &|| true)?;
    assert!(view.binary);
    restore(&f.0, &view, &[0], &|| true)?;
    assert_eq!(
        std::fs::read(f.0.join("file")).map_err(|e| e.to_string())?,
        b"\xff\xfe\x80binary without nul"
    );
    let view = preview(&f.0, &draft.id, "file", true, &|| true)?;
    restore(&f.0, &view, &[0], &|| true)?;
    assert_eq!(
        std::fs::read(f.0.join("file")).map_err(|e| e.to_string())?,
        b"staged\n"
    );
    let view = preview(&f.0, &draft.id, "empty", false, &|| true)?;
    restore(&f.0, &view, &[0], &|| true)?;
    assert_eq!(
        std::fs::metadata(f.0.join("empty"))
            .map_err(|e| e.to_string())?
            .len(),
        0
    );
    std::fs::remove_file(f.0.join("file")).map_err(|e| e.to_string())?;
    let deleted = capture(&f.0, "Deleted", &|| true)?.ok_or("no deletion draft")?;
    f.write("file", b"keep until confirmed")?;
    let view = preview(&f.0, &deleted.id, "file", false, &|| true)?;
    restore(&f.0, &view, &[0], &|| true)?;
    assert!(!f.0.join("file").exists());
    assert_eq!(
        std::fs::read(f.0.join(".git/index")).map_err(|e| e.to_string())?,
        index
    );
    Ok(())
}

#[test]
fn recovery_refuses_stale_files_indexes_refs_and_new_ignore_rules() -> Result<(), String> {
    let f = Fixture::new()?;
    f.write("file", b"base\n")?;
    f.commit()?;
    f.write("file", b"saved\n")?;
    f.write("new", b"old untracked")?;
    let draft = capture(&f.0, "Pause", &|| true)?.ok_or("no draft")?;
    f.write("file", b"current\n")?;
    let view = preview(&f.0, &draft.id, "file", false, &|| true)?;
    f.write("file", b"edited after preview\n")?;
    assert!(restore(&f.0, &view, &[0], &|| true).is_err());
    assert_eq!(
        std::fs::read(f.0.join("file")).map_err(|e| e.to_string())?,
        b"edited after preview\n"
    );
    let view = preview(&f.0, &draft.id, "file", false, &|| true)?;
    git::run(Some(&f.0), &["add", "file"])?;
    assert!(restore(&f.0, &view, &[0], &|| true).is_err());
    let view = preview(&f.0, &draft.id, "file", false, &|| true)?;
    git::run(Some(&f.0), &["update-ref", "-d", &draft.id.reference])?;
    assert!(restore(&f.0, &view, &[0], &|| true).is_err());
    git::run(
        Some(&f.0),
        &["update-ref", &draft.id.reference, &draft.id.tree],
    )?;
    f.write(".gitignore", b"new\n")?;
    assert!(preview(&f.0, &draft.id, "new", false, &|| true).is_err());
    assert_eq!(
        std::fs::read(f.0.join("new")).map_err(|e| e.to_string())?,
        b"old untracked"
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn partial_shallow_drafts_and_missing_index_preview_never_call_network_helpers()
-> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new()?;
    f.write("keep/file", b"keep base\n")?;
    f.write("omit/file", b"unavailable blob\n")?;
    f.commit()?;
    git::run(Some(&f.0), &["config", "uploadpack.allowFilter", "true"])?;
    let missing = git::run(Some(&f.0), &["rev-parse", "HEAD:omit/file"])?;
    let dest = f.0.join(".git/partial-clone");
    git::run(
        None,
        &[
            "clone",
            "--no-local",
            "--filter=blob:none",
            "--depth=1",
            "--no-checkout",
            "--",
            f.0.to_str().ok_or("utf8")?,
            dest.to_str().ok_or("utf8")?,
        ],
    )?;
    // Checkout is a passive operation under the shared no-lazy-fetch policy.
    // Seed only the known fixture blob needed for the sparse working folder;
    // the omitted promisor blob remains unavailable for the regression below.
    let keep = git::run(Some(&f.0), &["rev-parse", "HEAD:keep/file"])?;
    let seeded = git::run_input(
        Some(&dest),
        &["hash-object", "-w", "--stdin"],
        "keep base\n",
    )?;
    assert_eq!(seeded.trim(), keep.trim());
    git::run(Some(&dest), &["sparse-checkout", "set", "--cone", "keep"])?;
    git::run(Some(&dest), &["checkout", "-q"])?;
    assert_eq!(
        git::run(Some(&dest), &["rev-parse", "--is-shallow-repository"])?.trim(),
        "true"
    );
    assert!(
        git::run(
            Some(&dest),
            &["--no-lazy-fetch", "cat-file", "-e", missing.trim()]
        )
        .is_err()
    );
    let helper = f.0.join(".git/upload-helper");
    let marker = f.0.join(".git/network-helper-ran");
    std::fs::write(
        &helper,
        format!(
            "#!/bin/sh\ntouch {}\nexit 1\n",
            remote::sh_quote(marker.to_str().ok_or("utf8")?)
        ),
    )
    .map_err(|e| e.to_string())?;
    std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o700))
        .map_err(|e| e.to_string())?;
    git::run(
        Some(&dest),
        &[
            "config",
            "remote.origin.uploadpack",
            helper.to_str().ok_or("utf8")?,
        ],
    )?;
    std::fs::write(dest.join("keep/file"), b"working edit\n").map_err(|e| e.to_string())?;
    // A staged promisor blob is known by ID but has never been downloaded.
    git::run(
        Some(&dest),
        &[
            "update-index",
            "--cacheinfo",
            &format!("100644,{},keep/file", missing.trim()),
        ],
    )?;
    let objects = git::run(Some(&dest), &["count-objects", "-v"])?;
    let index = std::fs::read(dest.join(".git/index")).map_err(|e| e.to_string())?;
    let config = std::fs::read(dest.join(".git/config")).map_err(|e| e.to_string())?;
    let saved = capture(&dest, "Offline pause", &|| true)?.ok_or("No partial version")?;
    assert!(
        list(&dest, &|| true)?
            .iter()
            .any(|draft| draft.id == saved.id)
    );
    assert!(preview(&dest, &saved.id, "keep/file", true, &|| true).is_err());
    assert!(!marker.exists());
    assert_eq!(
        std::fs::read(dest.join(".git/index")).map_err(|e| e.to_string())?,
        index
    );
    assert_eq!(
        std::fs::read(dest.join(".git/config")).map_err(|e| e.to_string())?,
        config
    );
    // Snapshot objects are deliberately added; no existing promised pack is fetched.
    let after = git::run(Some(&dest), &["count-objects", "-v"])?;
    let packs = |text: &str| {
        text.lines()
            .filter(|line| line.starts_with("packs:") || line.starts_with("in-pack:"))
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    assert_eq!(packs(&after), packs(&objects));
    assert!(git::run(Some(&dest), &["cat-file", "-e", missing.trim()]).is_err());
    assert!(
        !marker.exists(),
        "Ordinary object reads must also refuse lazy fetch"
    );
    // An explicit fetch to this owned local source exercises the installed trap.
    // It is separate from draft inspection and never contacts an external server.
    assert!(
        git::run(
            Some(&dest),
            &[
                "fetch",
                "--no-tags",
                "--no-write-fetch-head",
                "origin",
                missing.trim()
            ]
        )
        .is_err()
    );
    assert!(
        marker.exists(),
        "Explicit local fetch positive control must attempt the configured helper"
    );
    Ok(())
}

mod turns;

#[cfg(unix)]
#[cfg(unix)]
#[cfg(unix)]
#[cfg(unix)]
mod index_safety;
mod path_safety;
#[cfg(unix)]
#[cfg(unix)]
mod retention;
