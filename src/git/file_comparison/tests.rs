use super::*;
use std::path::PathBuf;
use std::process::Command;

pub(crate) struct Repo(pub PathBuf);
impl Repo {
    pub(crate) fn new() -> Result<Self, String> {
        static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "gitcraft-file-views-test-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).map_err(|e| e.to_string())?;
        let r = Self(path);
        r.git(&["init", "-q", "--template=", "-b", "main"])?;
        for (k, v) in [
            ("user.name", "Fixture"),
            ("user.email", "fixture@example.invalid"),
            ("commit.gpgSign", "false"),
            ("core.autocrlf", "false"),
            ("core.filemode", "true"),
            ("core.hooksPath", "/nonexistent/gitcraft-file-view-hooks"),
        ] {
            r.git(&["config", k, v])?;
        }
        Ok(r)
    }
    pub(crate) fn command(&self, args: &[&str]) -> Command {
        let mut c = Command::new("git");
        crate::shell_path::apply(&mut c);
        c.arg("-C")
            .arg(&self.0)
            .args(args)
            .env("GIT_CONFIG_NOSYSTEM", "1");
        for key in [
            "GIT_DIR",
            "GIT_WORK_TREE",
            "GIT_COMMON_DIR",
            "GIT_INDEX_FILE",
            "GIT_CONFIG_COUNT",
            "GIT_CONFIG_PARAMETERS",
            "GIT_OBJECT_DIRECTORY",
            "GIT_ALTERNATE_OBJECT_DIRECTORIES",
        ] {
            c.env_remove(key);
        }
        c
    }
    pub(crate) fn git(&self, args: &[&str]) -> Result<String, String> {
        let o = self.command(args).output().map_err(|e| e.to_string())?;
        if o.status.success() {
            Ok(String::from_utf8_lossy(&o.stdout).trim_end().into())
        } else {
            Err(String::from_utf8_lossy(&o.stderr).into())
        }
    }
    pub(crate) fn write(&self, path: &str, data: impl AsRef<[u8]>) -> Result<(), String> {
        let path = self.0.join(path);
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
        }
        std::fs::write(path, data).map_err(|e| e.to_string())
    }
    pub(crate) fn commit(&self) -> Result<String, String> {
        self.git(&["add", "-A"])?;
        self.git(&["commit", "-qm", "fixture", "--allow-empty"])?;
        self.git(&["rev-parse", "HEAD"])
    }
    pub(crate) fn lens(&self, path: &str) -> Result<Lens, String> {
        read_lens(
            &self.0,
            &LensSpec {
                path: path.into(),
                previous: None,
            },
            &|| true,
        )
    }
}
impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The SSH process is a fixture-owned shell; no network or account configuration is used.
#[cfg(unix)]
#[test]
fn encoded_remote_readers_and_locked_staging_preserve_raw_files_and_existing_locks()
-> Result<(), String> {
    const CHILD: &str = "GITCRAFT_FILE_COMPARISON_SSH_FIXTURE";
    const NAME: &str = "git::file_comparison::tests::encoded_remote_readers_and_locked_staging_preserve_raw_files_and_existing_locks";
    if let Ok(path) = std::env::var(CHILD) {
        let encoded = crate::remote::encode(
            &crate::remote::Host::Ssh("file-comparison-fixture.invalid".into()),
            &path,
        );
        let spec = LensSpec {
            path: "new folder/quoted ' $(touch ESCAPED).txt".into(),
            previous: Some("old/a".into()),
        };
        let snapshot = read_lens(&encoded, &spec, &|| true)?;
        assert_eq!(snapshot.versions[0].bytes, b"base\n");
        assert_eq!(snapshot.versions[1].bytes, b"base\n");
        assert_eq!(snapshot.versions[2].bytes, b"working\n");
        let migration = read_migration(&encoded, &MigrationSource::Staged, &|| true)?;
        assert_eq!(migration.moves.len(), 1);
        assert_eq!(migration.moves[0].similarity, Some(100));
        let matrix = matrix(
            &encoded,
            &["HEAD".into(), "refs/heads/main".into()],
            "old",
            &|| true,
        )?;
        assert_eq!(matrix.paths, [b"old/a".to_vec()]);
        let patch = patch_header(&spec.path, Some("100644"), Some("100644"))
            + "@@ -1 +1 @@\n-base\n+working\n";
        let lock = PathBuf::from(&path).join(".git/index.lock");
        std::fs::write(&lock, b"another writer").map_err(|e| e.to_string())?;
        assert!(apply_lens_patch(&encoded, &snapshot, &patch, false).is_err());
        assert_eq!(
            std::fs::read(&lock).map_err(|e| e.to_string())?,
            b"another writer"
        );
        std::fs::remove_file(&lock).map_err(|e| e.to_string())?;
        apply_lens_patch(&encoded, &snapshot, &patch, false)?;
        let current = read_lens(&encoded, &spec, &|| true)?;
        assert_eq!(current.versions[1].bytes, b"working\n");
        assert_eq!(current.versions[2].bytes, b"working\n");
        apply_lens_patch(&encoded, &current, &patch, true)?;
        let before_race = read_lens(&encoded, &spec, &|| true)?;
        std::fs::write(
            PathBuf::from(&path).join("attribute-race-trigger"),
            b"armed",
        )
        .map_err(|e| e.to_string())?;
        assert!(
            apply_lens_patch(&encoded, &before_race, &patch, false)
                .is_err_and(|e| e.contains("changed") || e.contains("stale"))
        );
        assert_eq!(
            read_lens(&encoded, &spec, &|| true)?.versions[1].bytes,
            b"base\n"
        );
        std::fs::remove_file(PathBuf::from(&path).join(".gitattributes"))
            .map_err(|e| e.to_string())?;
        let stale = read_lens(&encoded, &spec, &|| true)?;
        std::fs::write(PathBuf::from(&path).join(&spec.path), b"newer\n")
            .map_err(|e| e.to_string())?;
        assert!(
            apply_lens_patch(&encoded, &stale, &patch, false).is_err_and(|e| e.contains("stale"))
        );
        assert!(!PathBuf::from(&path).join("ESCAPED").exists());
        assert!(!lock.exists());
        return Ok(());
    }
    let mut r = Repo::new()?;
    let renamed = r.0.with_file_name(format!(
        "{} remote's repo",
        r.0.file_name().unwrap_or_default().to_string_lossy()
    ));
    std::fs::rename(&r.0, &renamed).map_err(|e| e.to_string())?;
    r.0 = renamed;
    r.write("old/a", b"base\n")?;
    r.commit()?;
    r.git(&["mv", "old/a", "new-name"])?;
    std::fs::create_dir(r.0.join("new folder")).map_err(|e| e.to_string())?;
    r.git(&["mv", "new-name", "new folder/quoted ' $(touch ESCAPED).txt"])?;
    r.write("new folder/quoted ' $(touch ESCAPED).txt", b"working\n")?;
    let bin = r.0.join("fixture-bin");
    std::fs::create_dir(&bin).map_err(|e| e.to_string())?;
    let real_git = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
        .map(|p| p.join("git"))
        .find(|p| p.is_file())
        .ok_or("No fixture Git executable")?;
    let wrapped_git = bin.join("git");
    std::fs::write(&wrapped_git, format!(
        "#!/bin/sh\nfor arg do\nif [ \"$arg\" = apply ] && [ -f {trigger} ]; then printf '* filter=fixture\\n' > {attrs}; rm -f -- {trigger}; fi\ndone\nexec {git} \"$@\"\n",
        trigger = crate::remote::sh_quote(&r.0.join("attribute-race-trigger").to_string_lossy()),
        attrs = crate::remote::sh_quote(&r.0.join(".gitattributes").to_string_lossy()),
        git = crate::remote::sh_quote(&real_git.to_string_lossy()),
    )).map_err(|e| e.to_string())?;
    let ssh = bin.join("ssh");
    std::fs::write(&ssh, "#!/bin/sh\nexec /bin/sh\n").map_err(|e| e.to_string())?;
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&ssh, std::fs::Permissions::from_mode(0o700))
        .map_err(|e| e.to_string())?;
    std::fs::set_permissions(&wrapped_git, std::fs::Permissions::from_mode(0o700))
        .map_err(|e| e.to_string())?;
    let mut paths = vec![bin];
    paths.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    let out = Command::new(std::env::current_exe().map_err(|e| e.to_string())?)
        .args(["--exact", NAME, "--nocapture"])
        .env(CHILD, &r.0)
        .env(
            "PATH",
            std::env::join_paths(paths).map_err(|e| e.to_string())?,
        )
        .output()
        .map_err(|e| e.to_string())?;
    assert!(
        out.status.success(),
        "{} {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        r.git(&["show", ":0:new folder/quoted ' $(touch ESCAPED).txt"])?,
        "base"
    );
    assert_eq!(
        std::fs::read(r.0.join("new folder/quoted ' $(touch ESCAPED).txt"))
            .map_err(|e| e.to_string())?,
        b"newer\n"
    );
    Ok(())
}

#[test]
fn blobless_matrix_does_not_fetch_and_missing_blob_comparisons_fail_explicitly()
-> Result<(), String> {
    let source = Repo::new()?;
    source.write("folder/config", "configuration bytes\n")?;
    source.commit()?;
    source.git(&["config", "uploadpack.allowFilter", "true"])?;
    let partial = Repo(source.0.join("partial"));
    let out = Command::new("git")
        .args(["clone", "--no-checkout", "--no-local", "--filter=blob:none"])
        .arg(&source.0)
        .arg(&partial.0)
        .output()
        .map_err(|e| e.to_string())?;
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let missing = partial.git(&["rev-list", "--objects", "--all", "--missing=print"])?;
    assert!(missing.lines().any(|s| s.starts_with('?')));
    let matrix = matrix(
        &partial.0,
        &["HEAD".into(), "refs/heads/main".into()],
        "folder/",
        &|| true,
    )?;
    assert_eq!(matrix.paths, [b"folder/config".to_vec()]);
    let entry = matrix.columns[0]
        .files
        .get(b"folder/config".as_slice())
        .ok_or("missing matrix entry")?;
    assert!(blob(&partial.0, Some(entry), &|| true).is_err());
    assert_eq!(
        partial.git(&["rev-list", "--objects", "--all", "--missing=print"])?,
        missing
    );
    Ok(())
}

#[test]
fn migration_reads_renames_copies_scores_and_immutable_blob_sides() -> Result<(), String> {
    let r = Repo::new()?;
    r.write(
        "old/rename.txt",
        (0..40).map(|i| format!("line {i}\n")).collect::<String>(),
    )?;
    r.write("old/copy.txt", "copied unchanged content\n")?;
    let before = r.commit()?;
    r.git(&["mv", "old/rename.txt", "new-rename.txt"])?;
    r.write(
        "new-rename.txt",
        (0..40)
            .map(|i| format!("line {}\n", if i == 10 { 99 } else { i }))
            .collect::<String>(),
    )?;
    r.write("new-copy.txt", "copied unchanged content\n")?;
    r.git(&["add", "-A"])?;
    let m = read_migration(&r.0, &MigrationSource::Staged, &|| true)?;
    assert!(m.moves.iter().any(|m| !m.copy
        && m.before.path == b"old/rename.txt"
        && m.after.path == b"new-rename.txt"
        && m.similarity.is_some_and(|v| v > 50 && v < 100)));
    let copied = m
        .moves
        .iter()
        .find(|m| m.copy && m.after.path == b"new-copy.txt")
        .ok_or("copy")?;
    assert_eq!(copied.similarity, Some(100));
    let entry = copied.after.clone();
    let after = r.commit()?;
    r.write("new-copy.txt", "later\n")?;
    r.commit()?;
    assert_eq!(
        blob(&r.0, Some(&entry), &|| true)?.bytes,
        b"copied unchanged content\n"
    );
    assert_eq!(
        read_migration(&r.0, &MigrationSource::Range(before, after), &|| true)?
            .moves
            .len(),
        m.moves.len()
    );
    Ok(())
}
#[test]
fn unborn_additions_deletions_and_empty_reorganizations_are_explicit() -> Result<(), String> {
    let r = Repo::new()?;
    r.write("new.txt", "one\ntwo\n")?;
    let lens = r.lens("new.txt")?;
    assert!(lens.head.is_none());
    assert!(lens.versions[0].mode.is_none());
    assert!(lens.versions[1].mode.is_none());
    assert_eq!(lens.versions[2].bytes, b"one\ntwo\n");
    r.git(&["add", "new.txt"])?;
    let m = read_migration(&r.0, &MigrationSource::Staged, &|| true)?;
    assert!(m.moves.is_empty());
    assert_eq!(m.other_changes, 1);
    r.commit()?;
    std::fs::remove_file(r.0.join("new.txt")).map_err(|e| e.to_string())?;
    assert!(r.lens("new.txt")?.versions[2].mode.is_none());
    Ok(())
}
#[test]
fn matrix_distinguishes_equal_blobs_missing_modes_and_pins_ambiguous_branch_refs()
-> Result<(), String> {
    let r = Repo::new()?;
    r.write("folder/config", "same\n")?;
    r.write("folder/gone", "gone\n")?;
    let a = r.commit()?;
    r.git(&["branch", "release-a"])?;
    r.git(&["tag", "release-a"])?;
    r.git(&["rm", "folder/gone"])?;
    r.git(&["update-index", "--chmod=+x", "folder/config"])?;
    r.git(&["commit", "-qm", "mode"])?;
    r.git(&["branch", "release-b"])?;
    let m = matrix(
        &r.0,
        &["refs/heads/release-a".into(), "refs/heads/release-b".into()],
        "folder",
        &|| true,
    )?;
    let x = m.columns[0]
        .files
        .get(b"folder/config".as_slice())
        .ok_or("config")?;
    let y = m.columns[1]
        .files
        .get(b"folder/config".as_slice())
        .ok_or("config")?;
    assert_eq!(x.oid, y.oid);
    assert_eq!((&*x.mode, &*y.mode), ("100644", "100755"));
    assert!(!m.columns[1].files.contains_key(b"folder/gone".as_slice()));
    r.git(&["branch", "-f", "release-a", "release-b"])?;
    assert_eq!(m.columns[0].oid, a);
    assert_eq!(blob(&r.0, Some(x), &|| true)?.bytes, b"same\n");
    Ok(())
}
#[test]
fn snapshot_checks_refuse_changed_head_index_flags_working_bytes_and_filters() -> Result<(), String>
{
    let r = Repo::new()?;
    r.write("a", "old\n")?;
    r.commit()?;
    r.write("a", "staged\n")?;
    r.git(&["add", "a"])?;
    r.write("a", "working\n")?;
    let old = r.lens("a")?;
    let patch =
        patch_header("a", Some("100644"), Some("100644")) + "@@ -1 +1 @@\n-staged\n+working\n";
    r.write("a", "newer\n")?;
    assert!(apply_lens_patch(&r.0, &old, &patch, false).is_err_and(|e| e.contains("stale")));
    r.write("a", "working\n")?;
    let old = r.lens("a")?;
    r.git(&["update-index", "--assume-unchanged", "a"])?;
    assert!(apply_lens_patch(&r.0, &old, &patch, false).is_err_and(|e| e.contains("stale")));
    r.git(&["update-index", "--no-assume-unchanged", "a"])?;
    let old = r.lens("a")?;
    r.git(&["commit", "-qm", "index moved"])?;
    assert!(apply_lens_patch(&r.0, &old, &patch, false).is_err_and(|e| e.contains("stale")));
    r.write(".gitattributes", "a filter=fixture\n")?;
    assert!(r.lens("a")?.staging_warning.is_some());
    assert_eq!(r.git(&["show", ":0:a"])?, "staged");
    assert_eq!(
        std::fs::read(r.0.join("a")).map_err(|e| e.to_string())?,
        b"working\n"
    );
    Ok(())
}
#[test]
fn guarded_patch_keeps_working_bytes_and_other_index_entries_and_preserves_an_existing_lock()
-> Result<(), String> {
    let r = Repo::new()?;
    r.write("a", "old\n")?;
    r.write("other", "untouched\n")?;
    r.commit()?;
    r.write("a", "new\n")?;
    let lens = r.lens("a")?;
    let patch = patch_header("a", Some("100644"), Some("100644")) + "@@ -1 +1 @@\n-old\n+new\n";
    let lock = r.0.join(".git/index.lock");
    std::fs::write(&lock, "someone else").map_err(|e| e.to_string())?;
    assert!(apply_lens_patch(&r.0, &lens, &patch, false).is_err());
    assert_eq!(
        std::fs::read_to_string(&lock).map_err(|e| e.to_string())?,
        "someone else"
    );
    std::fs::remove_file(lock).map_err(|e| e.to_string())?;
    apply_lens_patch(&r.0, &lens, &patch, false)?;
    assert_eq!(r.git(&["show", ":0:a"])?, "new");
    assert_eq!(r.git(&["show", ":0:other"])?, "untouched");
    assert_eq!(
        std::fs::read(r.0.join("a")).map_err(|e| e.to_string())?,
        b"new\n"
    );
    let lens = r.lens("a")?;
    apply_lens_patch(&r.0, &lens, &patch, true)?;
    assert_eq!(r.git(&["show", ":0:a"])?, "old");
    assert!(!r.0.join(".git/index.lock").exists());
    Ok(())
}
#[test]
fn patch_paths_cannot_escape_the_snapshot_or_inject_an_additional_file() -> Result<(), String> {
    let r = Repo::new()?;
    r.write("a", "old\n")?;
    r.commit()?;
    r.write("a", "new\n")?;
    let lens = r.lens("a")?;
    let wrong = patch_header("other", Some("100644"), Some("100644")) + "@@ -1 +1 @@\n-old\n+new\n";
    assert!(apply_lens_patch(&r.0, &lens, &wrong, false).is_err());
    assert!(!valid_path(&r.0, "../outside"));
    assert!(!valid_path(&r.0, "/outside"));
    assert!(!valid_path(&r.0, "nested/../outside"));
    Ok(())
}
#[cfg(unix)]
#[test]
fn link_targets_and_binary_versions_are_read_without_following_links() -> Result<(), String> {
    let r = Repo::new()?;
    r.write("binary", b"\0\xfforiginal")?;
    std::os::unix::fs::symlink("/dev/zero", r.0.join("link")).map_err(|e| e.to_string())?;
    r.commit()?;
    let link = r.lens("link")?;
    assert_eq!(link.versions[2].mode.as_deref(), Some("120000"));
    assert_eq!(link.versions[2].bytes, b"/dev/zero");
    assert!(r.lens("binary")?.versions[2].text().is_none());
    std::os::unix::fs::symlink("/tmp", r.0.join("escape")).map_err(|e| e.to_string())?;
    assert!(r.lens("escape/anything")?.versions[2].warning.is_some());
    Ok(())
}
#[cfg(unix)]
#[test]
fn raw_non_utf8_paths_stay_distinct_and_literal_folders_do_not_expand_globs() -> Result<(), String>
{
    use std::os::unix::ffi::OsStringExt;
    let r = Repo::new()?;
    for bytes in [b"odd\xff".to_vec(), b"odd\xfe".to_vec()] {
        std::fs::write(r.0.join(std::ffi::OsString::from_vec(bytes)), "raw")
            .map_err(|e| e.to_string())?;
    }
    r.write("[folder]/file", "literal\n")?;
    r.commit()?;
    r.git(&["branch", "release"])?;
    let m = matrix(
        &r.0,
        &["HEAD".into(), "refs/heads/release".into()],
        "",
        &|| true,
    )?;
    assert!(m.paths.contains(&b"odd\xff".to_vec()));
    assert!(m.paths.contains(&b"odd\xfe".to_vec()));
    let m = matrix(
        &r.0,
        &["HEAD".into(), "refs/heads/release".into()],
        "[folder]",
        &|| true,
    )?;
    assert_eq!(m.paths, [b"[folder]/file".to_vec()]);
    Ok(())
}
#[test]
fn size_and_cancellation_limits_fail_explicitly() -> Result<(), String> {
    let r = Repo::new()?;
    r.write("large", vec![b'x'; FILE_LIMIT + 1])?;
    r.commit()?;
    assert!(r.lens("large").is_err_and(|e| e.contains("2 MiB")));
    assert!(
        read_lens(
            &r.0,
            &LensSpec {
                path: "large".into(),
                previous: None
            },
            &|| false
        )
        .is_err_and(|e| e.contains("cancelled"))
    );
    assert!(matrix(&r.0, &["HEAD".into()], "", &|| true).is_err());
    Ok(())
}

#[test]
fn missing_promised_attributes_fail_closed_without_starting_a_transport() -> Result<(), String> {
    const CHILD: &str = "GITCRAFT_MISSING_ATTRIBUTES_FIXTURE";
    const NAME: &str = "git::file_comparison::tests::missing_promised_attributes_fail_closed_without_starting_a_transport";
    if let Some(path) = std::env::var_os(CHILD) {
        let path = PathBuf::from(path);
        let file = std::env::var("GITCRAFT_MISSING_ATTRIBUTES_FILE").map_err(|e| e.to_string())?;
        let expected = if file.starts_with("nested/") {
            "nested/.gitattributes"
        } else {
            ".gitattributes"
        };
        let result = read_lens(
            &path,
            &LensSpec {
                path: file,
                previous: None,
            },
            &|| true,
        );
        assert!(
            result.is_err_and(|e| e.contains(expected)),
            "Missing conversion rules must be explicit and read-only"
        );
        return Ok(());
    }
    let source = Repo::new()?;
    source.write("file.txt", b"plain\n")?;
    source.write("nested/file.txt", b"plain\n")?;
    source.write(".gitattributes", b"file.txt text eol=lf\n")?;
    source.write("nested/.gitattributes", b"file.txt filter=fixture\n")?;
    source.commit()?;
    source.git(&["config", "uploadpack.allowFilter", "true"])?;
    let attr_oids = [
        source.git(&["rev-parse", "HEAD:.gitattributes"])?,
        source.git(&["rev-parse", "HEAD:nested/.gitattributes"])?,
    ];
    let partial = Repo(source.0.join("partial"));
    let out = source
        .command(&["clone", "--no-checkout", "--no-local", "--filter=blob:none"])
        .arg(&source.0)
        .arg(&partial.0)
        .output()
        .map_err(|e| e.to_string())?;
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    partial.git(&["--no-lazy-fetch", "read-tree", "HEAD"])?;
    partial.write("file.txt", b"plain\n")?;
    partial.write("nested/file.txt", b"plain\n")?;
    // Seed only the compared file blob. Both indexed attribute files stay promised
    // and absent, so Git's attribute fallback would otherwise fetch them silently.
    partial.git(&[
        "--no-lazy-fetch",
        "hash-object",
        "--no-filters",
        "-w",
        "file.txt",
    ])?;
    for oid in &attr_oids {
        assert!(
            partial
                .git(&["--no-lazy-fetch", "cat-file", "-e", oid])
                .is_err()
        );
    }
    for (i, file) in ["file.txt", "nested/file.txt"].into_iter().enumerate() {
        if i == 1 {
            // Make the root rule available for the second case, so the nested
            // attribute blob is the independently exercised unavailable source.
            let mut seed =
                partial.command(&["--no-lazy-fetch", "hash-object", "--no-filters", "-w"]);
            let out = seed
                .arg(source.0.join(".gitattributes"))
                .output()
                .map_err(|e| e.to_string())?;
            assert!(out.status.success());
        }
        let missing = partial.git(&[
            "--no-lazy-fetch",
            "rev-list",
            "--objects",
            "--all",
            "--missing=print",
        ])?;
        let trace = source.0.join(format!("transport-trace-{i}"));
        let out = Command::new(std::env::current_exe().map_err(|e| e.to_string())?)
            .args(["--exact", NAME, "--nocapture"])
            .env(CHILD, &partial.0)
            .env("GITCRAFT_MISSING_ATTRIBUTES_FILE", file)
            .env("GIT_TRACE", &trace)
            .output()
            .map_err(|e| e.to_string())?;
        assert!(
            out.status.success(),
            "{} {}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        let trace = std::fs::read_to_string(trace).map_err(|e| e.to_string())?;
        assert!(
            !trace.contains(" fetch ") && !trace.contains("upload-pack"),
            "No transport may start: {trace}"
        );
        assert!(
            partial
                .git(&["--no-lazy-fetch", "cat-file", "-e", &attr_oids[i]])
                .is_err()
        );
        assert_eq!(
            partial.git(&[
                "--no-lazy-fetch",
                "rev-list",
                "--objects",
                "--all",
                "--missing=print"
            ])?,
            missing
        );
    }
    Ok(())
}
