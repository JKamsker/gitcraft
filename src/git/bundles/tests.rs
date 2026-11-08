use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Result<Self, String> {
        static ID: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "gitcraft-bundle-test-{}-{}",
            std::process::id(),
            ID.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).map_err(|e| e.to_string())?;
        Ok(Self(root))
    }
    fn repo(&self, name: &str) -> Result<PathBuf, String> {
        self.repo_format(name, "sha1")
    }
    fn repo_format(&self, name: &str, format: &str) -> Result<PathBuf, String> {
        let path = self.0.join(name);
        std::fs::create_dir(&path).map_err(|e| e.to_string())?;
        for args in [
            vec!["init", "-q", "--object-format", format],
            vec!["config", "user.name", "Test"],
            vec!["config", "user.email", "test@example.com"],
            vec!["config", "commit.gpgSign", "false"],
        ] {
            run(Some(&path), &args)?;
        }
        Ok(path)
    }
    fn commit(&self, repo: &Path, text: &str) -> Result<String, String> {
        std::fs::write(repo.join("file"), text).map_err(|e| e.to_string())?;
        run(Some(repo), &["add", "file"])?;
        run(Some(repo), &["commit", "-qm", text])?;
        run(Some(repo), &["rev-parse", "HEAD"]).map(|s| s.trim().to_owned())
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn head(repo: &Path) -> Result<BundleRef, String> {
    bundle_refs(repo)?
        .into_iter()
        .find(|r| r.name == "HEAD")
        .ok_or_else(|| "no HEAD".into())
}
fn refs(repo: &Path) -> Result<String, String> {
    run(
        Some(repo),
        &["for-each-ref", "--format=%(refname) %(objectname)"],
    )
}

#[test]
fn windows_bundle_arguments_preserve_drive_unc_unicode_and_long_paths() -> Result<(), String> {
    for (native, expected) in [
        (
            r"\\?\C:\Users\a 'ü\exchange.bundle",
            "C:/Users/a 'ü/exchange.bundle",
        ),
        (
            r"\\?\UNC\Server\Share\目录\exchange.bundle",
            "//Server/Share/目录/exchange.bundle",
        ),
        (
            r"\\?\unc\Server\Share\link\..\exchange.bundle",
            "//Server/Share/link/../exchange.bundle",
        ),
        (
            r"\\Server\Share\exchange.bundle",
            "//Server/Share/exchange.bundle",
        ),
        (r"C:\Users\a\exchange.bundle", "C:/Users/a/exchange.bundle"),
        (
            "//?/C:/Users/a/exchange.bundle",
            "C:/Users/a/exchange.bundle",
        ),
    ] {
        assert_eq!(windows_bundle_path(native)?, expected);
    }
    let tail = format!("{}exchange.bundle", "deep ü/".repeat(80));
    assert_eq!(
        windows_bundle_path(&format!("\\\\?\\C:\\{tail}"))?,
        format!("C:/{tail}")
    );
    assert_eq!(
        windows_bundle_path(&format!("\\\\?\\UNC\\Server\\Share\\{tail}"))?,
        format!("//Server/Share/{tail}")
    );
    for unsupported in [
        r"\\?\Volume{fixture}\file",
        r"\\.\PhysicalDrive0",
        r"\\?\UNC\server",
    ] {
        assert!(windows_bundle_path(unsupported).is_err());
    }
    Ok(())
}

#[test]
fn native_long_bundle_paths_verify_exact_bytes_and_cleanup_without_changing_config()
-> Result<(), String> {
    let f = Fixture::new()?;
    let source = f.repo("source")?;
    f.commit(&source, "long-path fixture")?;
    let selected = vec![head(&source)?];
    let bytes = export_bundle(&source, &selected, None)?;
    let dest = f.repo("dest")?;
    let config_before = std::fs::read(dest.join(".git/config")).map_err(|e| e.to_string())?;
    let folder = if cfg!(windows) {
        "scratch ü ' $() &"
    } else {
        "scratch ü ' $() & \\?\\C:literal"
    };
    let mut root = f.0.join(folder);
    for _ in 0..7 {
        root.push("a long directory component with spaces and ü");
    }
    std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let root = std::fs::canonicalize(root).map_err(|e| e.to_string())?;
    let temp = BundleFile::local(&root, &bytes)?;
    let native = temp
        .local_dir
        .as_ref()
        .ok_or("No native scratch directory")?;
    let native = native.join("exchange.bundle");
    assert!(native.as_os_str().len() > 260);
    assert_eq!(std::fs::read(&native).map_err(|e| e.to_string())?, bytes);
    assert!(!temp.path.starts_with("//?/"));
    verify(&dest, &temp.path, &selected, &bytes)?;
    assert!(refs(&dest)?.is_empty());
    assert_eq!(
        std::fs::read(dest.join(".git/config")).map_err(|e| e.to_string())?,
        config_before
    );
    let private = temp.local_dir.clone().ok_or("No native cleanup path")?;
    drop(temp);
    assert!(!private.exists());
    assert!(root.is_dir());
    Ok(())
}

#[cfg(unix)]
#[test]
fn remote_binary_request_quotes_paths_refs_and_stdin_as_data() -> Result<(), String> {
    use std::process::{Command, Stdio};
    let f = Fixture::new()?;
    let source = f.repo("source ' $(touch injected)")?;
    f.commit(&source, "one")?;
    // Git ref names can contain shell punctuation (but not spaces).
    let name = "refs/heads/quote'$(touch-injected)";
    run(Some(&source), &["update-ref", name, "HEAD"])?;
    let target = remote::Target {
        host: remote::Host::Ssh("test".into()),
        path: source.display().to_string(),
    };
    let req = bounded::request(
        &target,
        &["bundle", "create", "--version=3", "-", name],
        vec![],
        BUNDLE_LIMIT,
    );
    let mut child = Command::new("sh")
        .current_dir(&source)
        .arg("-c")
        .arg(&req.script)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .envs(req.env)
        .spawn()
        .map_err(|e| e.to_string())?;
    child
        .stdin
        .take()
        .ok_or("no input")?
        .write_all(&req.stdin.unwrap_or_default())
        .map_err(|e| e.to_string())?;
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let (refs, _) = header(&out.stdout)?;
    assert_eq!(refs[0].name, name);
    assert!(!source.join("injected").exists());
    let req = bounded::request(
        &target,
        &["hash-object", "--stdin"],
        b"$(touch injected)\n'quoted'".to_vec(),
        1024,
    );
    let mut child = Command::new("sh")
        .current_dir(&source)
        .arg("-c")
        .arg(&req.script)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    child
        .stdin
        .take()
        .ok_or("no input")?
        .write_all(&req.stdin.unwrap_or_default())
        .map_err(|e| e.to_string())?;
    assert!(
        child
            .wait_with_output()
            .map_err(|e| e.to_string())?
            .status
            .success()
    );
    assert!(!source.join("injected").exists());
    Ok(())
}

#[test]
fn regular_file_size_limits_and_private_temp_cleanup_are_enforced() -> Result<(), String> {
    let f = Fixture::new()?;
    let source = f.repo("source")?;
    f.commit(&source, "one")?;
    let bytes = export_bundle(&source, &[head(&source)?], None)?;
    let temp = BundleFile::new(&source, &bytes)?;
    let file = PathBuf::from(&temp.path);
    let parent = file.parent().ok_or("no temp parent")?.to_owned();
    assert_eq!(read_bundle_file(&file)?, bytes);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&parent)
                .map_err(|e| e.to_string())?
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
    }
    drop(temp);
    assert!(!parent.exists());
    let large = f.0.join("too-large.bundle");
    std::fs::File::create(&large)
        .and_then(|f| f.set_len(BUNDLE_LIMIT as u64 + 1))
        .map_err(|e| e.to_string())?;
    assert!(read_bundle_file(&large).is_err());
    #[cfg(unix)]
    {
        let link = f.0.join("linked.bundle");
        std::os::unix::fs::symlink(&large, &link).map_err(|e| e.to_string())?;
        assert!(read_bundle_file(&link).is_err());
        assert!(save_bundle_file(&link, b"do not overwrite").is_err());
        assert_eq!(
            std::fs::metadata(&large).map_err(|e| e.to_string())?.len(),
            BUNDLE_LIMIT as u64 + 1
        );
    }
    let output = bounded::run(&source, &["show", "HEAD:file"], vec![], 2);
    assert!(output.is_err());
    Ok(())
}

#[cfg(unix)]
#[test]
fn blocked_git_children_and_input_writers_obey_the_deadline() -> Result<(), String> {
    use std::time::{Duration, Instant};
    let f = Fixture::new()?;
    let repo = f.repo("source")?;
    for alias in ["alias.hung=!sleep 30", "alias.hung=!sleep 30 & exit 0"] {
        let start = Instant::now();
        let result = bounded::local(
            &repo,
            &["-c", alias, "hung"],
            vec![b'x'; 1024 * 1024],
            1024,
            Duration::from_millis(100),
        );
        assert!(result.is_err());
        assert!(start.elapsed() < Duration::from_secs(3));
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn partial_shallow_bundle_preview_never_fetches_missing_prerequisites() -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new()?;
    let source = f.repo("source")?;
    run(Some(&source), &["config", "uploadpack.allowFilter", "true"])?;
    run(
        Some(&source),
        &["config", "uploadpack.allowAnySHA1InWant", "true"],
    )?;
    let base = f.commit(&source, "base")?;
    f.commit(&source, "tip")?;
    let bytes = export_bundle(&source, &[head(&source)?], Some(&base))?;
    let bundle = f.0.join("incremental.bundle");
    save_bundle_file(&bundle, &bytes)?;
    let dest = f.0.join("partial");
    run(
        None,
        &[
            "-c",
            "protocol.file.allow=always",
            "clone",
            "--no-local",
            "--filter=blob:none",
            "--depth=1",
            "--",
            source.to_str().ok_or("source path")?,
            dest.to_str().ok_or("destination path")?,
        ],
    )?;
    run(Some(&dest), &["config", "protocol.file.allow", "always"])?;
    assert_eq!(
        run(Some(&dest), &["rev-parse", "--is-shallow-repository"])?.trim(),
        "true"
    );
    assert_eq!(
        run(Some(&dest), &["config", "--get", "remote.origin.promisor"])?.trim(),
        "true"
    );
    assert!(run(Some(&dest), &["--no-lazy-fetch", "cat-file", "-e", &base]).is_err());
    let marker = f.0.join("network-helper-ran");
    let upload = f.0.join("upload-pack");
    std::fs::write(
        &upload,
        format!(
            "#!/bin/sh\nprintf 'fetch\\n' >> {}\nexec git-upload-pack \"$@\"\n",
            remote::sh_quote(marker.to_str().ok_or("marker path")?)
        ),
    )
    .map_err(|e| e.to_string())?;
    std::fs::set_permissions(&upload, std::fs::Permissions::from_mode(0o700))
        .map_err(|e| e.to_string())?;
    run(
        Some(&dest),
        &[
            "config",
            "remote.origin.uploadpack",
            upload.to_str().ok_or("helper path")?,
        ],
    )?;
    let object_store = run(Some(&dest), &["count-objects", "-v"])?;
    let refs_before = refs(&dest)?;
    let index = std::fs::read(dest.join(".git/index")).map_err(|e| e.to_string())?;
    let config = std::fs::read(dest.join(".git/config")).map_err(|e| e.to_string())?;
    let shallow = std::fs::read(dest.join(".git/shallow")).map_err(|e| e.to_string())?;
    let preview = inspect_bundle(&dest, bytes)?;
    let supports_no_lazy = no_lazy_fetch(&dest).is_ok();
    assert!(
        preview
            .verification_error
            .as_ref()
            .is_some_and(|e| if supports_no_lazy {
                e.contains(&base)
            } else {
                e.contains("Upgrade Git")
            }),
        "{:?}",
        preview.verification_error
    );
    assert_eq!(preview.prerequisites[0].oid, base);
    // The shared runner keeps ordinary verification passive too.
    assert!(
        run(
            Some(&dest),
            &["bundle", "verify", bundle.to_str().ok_or("bundle path")?],
        )
        .is_err()
    );
    assert_eq!(run(Some(&dest), &["count-objects", "-v"])?, object_store);
    assert_eq!(refs(&dest)?, refs_before);
    assert!(!marker.exists());
    assert_eq!(run(Some(&dest), &["count-objects", "-v"])?, object_store);
    assert_eq!(refs(&dest)?, refs_before);
    assert_eq!(
        std::fs::read(dest.join(".git/index")).map_err(|e| e.to_string())?,
        index
    );
    assert_eq!(
        std::fs::read(dest.join(".git/config")).map_err(|e| e.to_string())?,
        config
    );
    assert_eq!(
        std::fs::read(dest.join(".git/shallow")).map_err(|e| e.to_string())?,
        shallow
    );
    assert_eq!(
        std::fs::read_to_string(dest.join("file")).map_err(|e| e.to_string())?,
        "tip"
    );
    assert!(!marker.exists());
    if !supports_no_lazy {
        return Ok(());
    }
    // Explicitly fetch this prerequisite from the owned local origin. The helper
    // serves real objects; subsequent verification must need no further fetch.
    run(
        Some(&dest),
        &[
            "fetch",
            "--no-tags",
            "--no-write-fetch-head",
            "origin",
            &base,
        ],
    )?;
    assert!(marker.exists());
    assert_ne!(run(Some(&dest), &["count-objects", "-v"])?, object_store);
    let calls = std::fs::read(&marker).map_err(|e| e.to_string())?;
    run(
        Some(&dest),
        &["bundle", "verify", bundle.to_str().ok_or("bundle path")?],
    )?;
    assert_eq!(std::fs::read(&marker).map_err(|e| e.to_string())?, calls);
    assert_eq!(refs(&dest)?, refs_before);
    assert_eq!(
        std::fs::read(dest.join(".git/config")).map_err(|e| e.to_string())?,
        config
    );
    assert_eq!(
        std::fs::read(dest.join(".git/shallow")).map_err(|e| e.to_string())?,
        shallow
    );
    assert_eq!(
        std::fs::read_to_string(dest.join("file")).map_err(|e| e.to_string())?,
        "tip"
    );
    assert_eq!(
        std::fs::read(dest.join(".git/index")).map_err(|e| e.to_string())?,
        index
    );
    Ok(())
}

#[test]
fn old_git_verification_is_fail_closed_for_promisor_or_unreadable_configuration()
-> Result<(), String> {
    let unsupported = || Err("unknown option: --no-lazy-fetch\nusage: git ...".to_owned());
    assert_eq!(
        no_lazy_compatibility(Ok(vec![]), || Err(
            "must not read config for supported Git".into()
        ))?,
        vec!["--no-lazy-fetch"]
    );
    assert!(no_lazy_compatibility(unsupported(), || Ok(String::new()))?.is_empty());
    for config in [
        "remote.origin.promisor true",
        "remote.origin.promisor false",
        "extensions.partialclone origin",
    ] {
        assert!(
            no_lazy_compatibility(unsupported(), || Ok(config.into()))
                .is_err_and(|e| e.contains("will not be fetched automatically"))
        );
    }
    assert!(no_lazy_compatibility(unsupported(), || Err("cannot read config".into())).is_err());
    assert!(no_lazy_compatibility(Err("SSH failed".into()), || Ok(String::new())).is_err());
    Ok(())
}

#[test]
fn verification_streams_only_the_validated_header_and_keeps_the_old_git_fallback()
-> Result<(), String> {
    let oid = "a".repeat(40);
    let header = format!("# v2 git bundle\n{oid} refs/heads/main\n\n");
    let mut bytes = header.as_bytes().to_vec();
    bytes.extend_from_slice(b"PACK");
    bytes.extend(vec![0; 1024 * 1024]);
    let long_path = format!("C:/{}exchange.bundle", "deep ü/".repeat(80));
    let (args, input) = verification_command(true, &["--no-lazy-fetch"], &long_path, &bytes)?;
    assert_eq!(
        args,
        [
            "--no-lazy-fetch",
            "-c",
            "core.longpaths=true",
            "bundle",
            "verify",
            "-"
        ]
    );
    assert_eq!(input, header.as_bytes());
    let (args, input) = verification_command(true, &[], &long_path, &bytes)?;
    assert_eq!(args.last().copied(), Some(long_path.as_str()));
    assert!(input.is_empty());
    let (args, input) = verification_command(false, &["--no-lazy-fetch"], &long_path, &bytes)?;
    assert_eq!(args.last().copied(), Some(long_path.as_str()));
    assert!(input.is_empty());
    assert!(verification_command(true, &["--no-lazy-fetch"], &long_path, b"not a bundle").is_err());
    Ok(())
}

#[test]
fn preview_never_unpacks_even_a_corrupt_pack() -> Result<(), String> {
    let f = Fixture::new()?;
    let source = f.repo("inspection-source")?;
    f.commit(&source, "one")?;
    let mut bytes = export_bundle(&source, &[head(&source)?], None)?;
    let end = bytes
        .windows(2)
        .position(|b| b == b"\n\n")
        .ok_or("header")?
        + 2;
    bytes.truncate(end);
    bytes.extend_from_slice(b"PACK deliberately corrupt: inspection must not unpack");
    let dest = f.repo("inspection-dest")?;
    let before = run(Some(&dest), &["count-objects", "-v"])?;
    let preview = inspect_bundle(&dest, bytes)?;
    assert!(
        preview.verification_error.is_none(),
        "{:?}",
        preview.verification_error
    );
    assert_eq!(preview.refs.len(), 1);
    assert_eq!(run(Some(&dest), &["count-objects", "-v"])?, before);
    assert!(refs(&dest)?.is_empty());
    Ok(())
}

#[test]
fn local_ref_validation_matches_native_git_without_remote_round_trips() {
    for name in [
        "refs/heads/main",
        "refs/tags/ü",
        "refs/heads/quote'$(literal)",
        "refs/heads/a@b",
        "refs/heads/has]bracket",
        "refs/heads/a.lock/x",
        "refs/heads/.hidden",
        "refs/heads/a..b",
        "refs/heads/a@{b",
        "refs/heads/a[",
        "refs/heads/a ",
        "refs/heads/a\\b",
        "refs/heads/a.",
        "refs//head",
        "refs/heads/x/",
        "refs/heads/a.lock",
    ] {
        assert_eq!(
            valid_ref(name).is_ok(),
            run(None, &["check-ref-format", name]).is_ok(),
            "{name}"
        );
    }
}
#[test]
fn export_ref_snapshots_and_saved_files_cannot_be_overwritten() -> Result<(), String> {
    let f = Fixture::new()?;
    let source = f.repo("source")?;
    let oid = f.commit(&source, "one")?;
    let selected = head(&source)?;
    let bytes = export_bundle(&source, std::slice::from_ref(&selected), None)?;
    f.commit(&source, "two")?;
    assert!(export_bundle(&source, &[selected], None).is_err());
    let out = f.0.join("quoted ' output.bundle");
    save_bundle_file(&out, &bytes)?;
    assert!(save_bundle_file(&out, b"replace").is_err());
    let snapshot = read_bundle_file(&out)?;
    assert_eq!(snapshot, bytes);
    std::fs::write(&out, b"replaced after preview").map_err(|e| e.to_string())?;
    let dest = f.repo("dest")?;
    let preview = inspect_bundle(&dest, snapshot)?;
    assert_eq!(preview.refs[0].oid, oid);
    assert!(refs(&dest)?.is_empty());
    Ok(())
}
#[test]
fn malformed_headers_and_untrusted_refs_are_rejected_without_unpacking() -> Result<(), String> {
    let f = Fixture::new()?;
    let source = f.repo("source")?;
    f.commit(&source, "one")?;
    let bytes = export_bundle(&source, &[head(&source)?], None)?;
    let oid = head(&source)?.oid;
    let pack = &bytes[bytes
        .windows(2)
        .position(|s| s == b"\n\n")
        .ok_or("no pack")?
        + 2..];
    for description in ["", "\tTabbed subject"] {
        let mut sample =
            format!("# v2 git bundle\n-{oid}{description}\n{oid} HEAD\n\n").into_bytes();
        // A tabbed subject uses the normal space after the prerequisite OID.
        if !description.is_empty() {
            sample = format!("# v2 git bundle\n-{oid} {description}\n{oid} HEAD\n\n").into_bytes();
        }
        sample.extend(pack);
        let (_, prerequisites) = header(&sample)?;
        assert_eq!(prerequisites.len(), 1);
        assert!(!prerequisites[0].name.chars().any(char::is_control));
    }
    let dest = f.repo("dest")?;
    for bad in [
        b"not a bundle".to_vec(),
        b"# v2 git bundle\n\nPACK".to_vec(),
        bytes[..20].to_vec(),
    ] {
        assert!(inspect_bundle(&dest, bad).is_err());
    }
    for name in [
        "--output=evil",
        "refs/heads/a b",
        "refs/heads/../escape",
        "refs/heads/a\ncreate refs/heads/evil",
        "refs/heads/a.lock",
    ] {
        let bad = String::from_utf8_lossy(&bytes).replacen(" HEAD\n", &format!(" {name}\n"), 1);
        assert!(inspect_bundle(&dest, bad.into_bytes()).is_err(), "{name}");
    }
    assert!(refs(&dest)?.is_empty());
    Ok(())
}

#[test]
fn unicode_whitespace_refs_and_legacy_prerequisite_subjects_are_preserved() -> Result<(), String> {
    let f = Fixture::new()?;
    let source = f.repo("unicode-refs")?;
    let oid = f.commit(&source, "one")?;
    let name = "refs/heads/nonbreaking\u{a0}space";
    run(Some(&source), &["update-ref", name, &oid])?;
    let selected = bundle_refs(&source)?
        .into_iter()
        .find(|r| r.name == name)
        .ok_or("Unicode whitespace ref missing")?;
    let bytes = export_bundle(&source, &[selected], None)?;
    assert_eq!(header(&bytes)?.0[0].name, name);
    let sample = [
        format!("# v2 git bundle\n-{oid} legacy ").into_bytes(),
        vec![0xe9],
        format!("\n{oid} HEAD\n\nPACK").into_bytes(),
    ]
    .concat();
    let (_, prerequisites) = header(&sample)?;
    assert_eq!(prerequisites[0].oid, oid);
    assert!(prerequisites[0].name.starts_with("legacy "));
    let bad_ref = [
        format!("# v2 git bundle\n{oid} refs/heads/").into_bytes(),
        vec![0xe9],
        b"\n\nPACK".to_vec(),
    ]
    .concat();
    assert!(header(&bad_ref).is_err());
    Ok(())
}

#[cfg(unix)]
#[test]
fn partial_clone_export_never_fetches_promised_blobs() -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new()?;
    let source = f.repo("offline-export-source")?;
    run(Some(&source), &["config", "uploadpack.allowFilter", "true"])?;
    run(
        Some(&source),
        &["config", "uploadpack.allowAnySHA1InWant", "true"],
    )?;
    f.commit(&source, "promised blob")?;
    let missing = run(Some(&source), &["rev-parse", "HEAD:file"])?;
    let dest = f.0.join("offline-export-dest");
    run(
        None,
        &[
            "-c",
            "protocol.file.allow=always",
            "clone",
            "--no-local",
            "--no-checkout",
            "--filter=blob:none",
            "--",
            source.to_str().ok_or("source")?,
            dest.to_str().ok_or("destination")?,
        ],
    )?;
    run(Some(&dest), &["config", "protocol.file.allow", "always"])?;
    let marker = f.0.join("export-fetch-ran");
    let helper = f.0.join("export-upload-pack");
    std::fs::write(
        &helper,
        format!(
            "#!/bin/sh\nprintf fetch >> {}\nexec git-upload-pack \"$@\"\n",
            remote::sh_quote(marker.to_str().ok_or("marker")?)
        ),
    )
    .map_err(|e| e.to_string())?;
    std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o700))
        .map_err(|e| e.to_string())?;
    run(
        Some(&dest),
        &[
            "config",
            "remote.origin.uploadpack",
            helper.to_str().ok_or("helper")?,
        ],
    )?;
    let before = run(Some(&dest), &["count-objects", "-v"])?;
    let selected = head(&dest)?;
    assert!(export_bundle(&dest, std::slice::from_ref(&selected), None).is_err());
    assert!(run(Some(&dest), &["cat-file", "-e", missing.trim()]).is_err());
    assert!(!marker.exists());
    assert_eq!(run(Some(&dest), &["count-objects", "-v"])?, before);
    // Only this explicit fetch may invoke the helper serving our local origin.
    run(
        Some(&dest),
        &[
            "fetch",
            "--no-tags",
            "--no-write-fetch-head",
            "origin",
            missing.trim(),
        ],
    )?;
    assert!(marker.exists(), "Explicit local fetch must call the helper");
    assert_ne!(run(Some(&dest), &["count-objects", "-v"])?, before);
    let calls = std::fs::read(&marker).map_err(|e| e.to_string())?;
    run(Some(&dest), &["cat-file", "-e", missing.trim()])?;
    let output = f.0.join("positive-control.bundle");
    run(
        Some(&dest),
        &["bundle", "create", output.to_str().ok_or("output")?, "HEAD"],
    )?;
    run(
        Some(&dest),
        &["bundle", "verify", output.to_str().ok_or("output")?],
    )?;
    assert_eq!(std::fs::read(&marker).map_err(|e| e.to_string())?, calls);
    Ok(())
}
