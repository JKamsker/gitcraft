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
fn mapping(destination: &str) -> BundleImport {
    BundleImport {
        source: "HEAD".into(),
        destination: destination.into(),
    }
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

#[test]
fn full_bundle_preview_is_read_only_and_import_creates_refs_without_checkout() -> Result<(), String>
{
    let f = Fixture::new()?;
    let source = f.repo("source")?;
    let oid = f.commit(&source, "one")?;
    run(Some(&source), &["tag", "-a", "v1", "-m", "annotated"])?;
    let selected: Vec<_> = bundle_refs(&source)?
        .into_iter()
        .filter(|r| r.name == "HEAD" || r.name == "refs/tags/v1")
        .collect();
    let bytes = export_bundle(&source, &selected, None)?;
    let dest = f.repo("dest")?;
    let before = refs(&dest)?;
    let preview = inspect_bundle(&dest, bytes)?;
    assert!(
        preview.verification_error.is_none(),
        "{:?}",
        preview.verification_error
    );
    assert!(preview.prerequisites.is_empty());
    assert_eq!(refs(&dest)?, before);
    assert!(run(Some(&dest), &["cat-file", "-e", &oid]).is_err());
    import_bundle(
        &dest,
        &preview,
        &[
            mapping("refs/heads/imported"),
            BundleImport {
                source: "refs/tags/v1".into(),
                destination: "refs/tags/imported-v1".into(),
            },
        ],
    )?;
    assert_eq!(
        run(Some(&dest), &["rev-parse", "refs/heads/imported"])?.trim(),
        oid
    );
    assert_eq!(
        run(Some(&dest), &["cat-file", "-t", "refs/tags/imported-v1"])?.trim(),
        "tag"
    );
    assert!(!dest.join("file").exists());
    assert!(!dest.join(".git/FETCH_HEAD").exists());
    assert!(run(Some(&dest), &["rev-parse", "--verify", "HEAD"]).is_err());
    Ok(())
}

#[test]
fn incremental_bundle_explains_missing_prerequisites_then_imports_after_base() -> Result<(), String>
{
    let f = Fixture::new()?;
    let source = f.repo("source")?;
    let base = f.commit(&source, "base\twith tab")?;
    let full = export_bundle(&source, &[head(&source)?], None)?;
    let tip = f.commit(&source, "tip")?;
    let incremental = export_bundle(&source, &[head(&source)?], Some(&base))?;
    let dest = f.repo("dest")?;
    let preview = inspect_bundle(&dest, incremental)?;
    assert_eq!(preview.prerequisites[0].oid, base);
    assert!(
        preview
            .verification_error
            .as_ref()
            .is_some_and(|e| e.contains(&base))
    );
    assert!(import_bundle(&dest, &preview, &[mapping("refs/heads/tip")]).is_err());
    assert!(refs(&dest)?.is_empty());
    let base_preview = inspect_bundle(&dest, full)?;
    import_bundle(&dest, &base_preview, &[mapping("refs/heads/base")])?;
    // Import re-verifies the pinned preview even if its earlier prerequisite check failed.
    import_bundle(&dest, &preview, &[mapping("refs/heads/tip")])?;
    assert_eq!(
        run(Some(&dest), &["rev-parse", "refs/heads/tip"])?.trim(),
        tip
    );
    Ok(())
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
    import_bundle(&dest, &preview, &[mapping("refs/heads/old-snapshot")])?;
    assert_eq!(
        run(Some(&dest), &["rev-parse", "refs/heads/old-snapshot"])?.trim(),
        oid
    );
    Ok(())
}

#[test]
fn import_collisions_and_concurrent_ref_changes_fail_atomically() -> Result<(), String> {
    let f = Fixture::new()?;
    let source = f.repo("source")?;
    f.commit(&source, "source")?;
    let bytes = export_bundle(&source, &[head(&source)?], None)?;
    let dest = f.repo("dest")?;
    let old = f.commit(&dest, "destination")?;
    let current = run(Some(&dest), &["symbolic-ref", "HEAD"])?;
    let preview = inspect_bundle(&dest, bytes)?;
    let before = refs(&dest)?;
    assert!(
        import_bundle(
            &dest,
            &preview,
            &[mapping("refs/heads/new"), mapping(current.trim())]
        )
        .is_err()
    );
    assert_eq!(refs(&dest)?, before);
    assert!(!dest.join(".git/refs/heads/new").exists());
    run(Some(&dest), &["update-ref", "refs/heads/raced", &old])?;
    let after_race = refs(&dest)?;
    assert!(
        import_bundle(
            &dest,
            &preview,
            &[
                mapping("refs/heads/another-new"),
                mapping("refs/heads/raced")
            ]
        )
        .is_err()
    );
    assert_eq!(refs(&dest)?, after_race);
    assert_eq!(
        std::fs::read_to_string(dest.join("file")).map_err(|e| e.to_string())?,
        "destination"
    );
    assert!(import_bundle(&dest, &preview, &[mapping("HEAD")]).is_err());
    assert!(import_bundle(&dest, &preview, &[mapping("refs/replace/evil")]).is_err());
    Ok(())
}

#[test]
fn symbolic_and_namespace_collision_destinations_are_not_followed() -> Result<(), String> {
    let f = Fixture::new()?;
    let source = f.repo("source")?;
    f.commit(&source, "one")?;
    let dest = f.repo("dest")?;
    let preview = inspect_bundle(&dest, export_bundle(&source, &[head(&source)?], None)?)?;
    run(
        Some(&dest),
        &["symbolic-ref", "refs/heads/alias", "refs/heads/missing"],
    )?;
    assert!(import_bundle(&dest, &preview, &[mapping("refs/heads/alias")]).is_err());
    assert!(
        run(
            Some(&dest),
            &["rev-parse", "--verify", "refs/heads/missing"]
        )
        .is_err()
    );
    import_bundle(&dest, &preview, &[mapping("refs/heads/prefix")])?;
    assert!(
        import_bundle(
            &dest,
            &preview,
            &[
                mapping("refs/heads/prefix/child"),
                mapping("refs/heads/other")
            ]
        )
        .is_err()
    );
    assert!(run(Some(&dest), &["rev-parse", "--verify", "refs/heads/other"]).is_err());
    Ok(())
}

#[test]
fn malformed_headers_untrusted_refs_and_corrupt_packs_never_create_refs() -> Result<(), String> {
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
    let mut corrupt = bytes;
    let last = corrupt.len() - 1;
    corrupt[last] ^= 1;
    let preview = inspect_bundle(&dest, corrupt)?;
    assert!(import_bundle(&dest, &preview, &[mapping("refs/heads/corrupt")]).is_err());
    assert!(refs(&dest)?.is_empty());
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
fn v2_and_sha256_bundles_preserve_object_formats() -> Result<(), String> {
    let f = Fixture::new()?;
    let source = f.repo("source")?;
    f.commit(&source, "one")?;
    let v2 = bounded::run(
        &source,
        &["bundle", "create", "--version=2", "-", "HEAD"],
        vec![],
        BUNDLE_LIMIT,
    )?;
    let dest = f.repo("dest")?;
    let preview = inspect_bundle(&dest, v2)?;
    assert!(preview.verification_error.is_none());
    import_bundle(&dest, &preview, &[mapping("refs/heads/v2")])?;
    let sha_source = f.repo_format("sha-source", "sha256")?;
    let oid = f.commit(&sha_source, "sha256")?;
    assert_eq!(oid.len(), 64);
    let sha_bundle = export_bundle(&sha_source, &[head(&sha_source)?], None)?;
    let wrong_format = inspect_bundle(&dest, sha_bundle.clone())?;
    assert!(wrong_format.verification_error.is_some());
    assert!(import_bundle(&dest, &wrong_format, &[mapping("refs/heads/wrong")]).is_err());
    let sha_dest = f.repo_format("sha-dest", "sha256")?;
    let preview = inspect_bundle(&sha_dest, sha_bundle)?;
    assert!(preview.verification_error.is_none());
    import_bundle(&sha_dest, &preview, &[mapping("refs/heads/sha256")])?;
    assert_eq!(
        run(Some(&sha_dest), &["rev-parse", "refs/heads/sha256"])?.trim(),
        oid
    );
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

#[test]
fn malformed_objects_with_a_valid_pack_checksum_are_rejected_before_refs() -> Result<(), String> {
    let f = Fixture::new()?;
    let source = f.repo("source")?;
    // A valid pack checksum alone is insufficient: native bundle unbundle accepts this.
    let oid = bounded::run(
        &source,
        &["hash-object", "--literally", "-w", "-t", "tree", "--stdin"],
        b"not-a-valid-tree".to_vec(),
        1024,
    )?;
    let pack = bounded::run(
        &source,
        &["pack-objects", "--stdout"],
        oid.clone(),
        BUNDLE_LIMIT,
    )?;
    let oid = std::str::from_utf8(&oid).map_err(|e| e.to_string())?.trim();
    let mut bytes = format!("# v2 git bundle\n{oid} refs/tags/malformed\n\n").into_bytes();
    bytes.extend(pack);
    let dest = f.repo("dest")?;
    let preview = inspect_bundle(&dest, bytes)?;
    assert!(preview.verification_error.is_none());
    let result = import_bundle(
        &dest,
        &preview,
        &[BundleImport {
            source: "refs/tags/malformed".into(),
            destination: "refs/tags/imported".into(),
        }],
    );
    assert!(result.is_err());
    assert!(refs(&dest)?.is_empty());
    Ok(())
}

#[test]
fn import_cannot_create_the_checked_out_unborn_branch() -> Result<(), String> {
    let f = Fixture::new()?;
    let source = f.repo("source")?;
    f.commit(&source, "one")?;
    let dest = f.repo("unborn")?;
    let current = run(Some(&dest), &["symbolic-ref", "HEAD"])?;
    let preview = inspect_bundle(&dest, export_bundle(&source, &[head(&source)?], None)?)?;
    assert!(preview.existing_refs.contains(current.trim()));
    assert!(import_bundle(&dest, &preview, &[mapping(current.trim())]).is_err());
    assert!(refs(&dest)?.is_empty());
    assert!(run(Some(&dest), &["rev-parse", "--verify", "HEAD"]).is_err());
    import_bundle(&dest, &preview, &[mapping("refs/heads/safe-backup")])?;
    assert!(run(Some(&dest), &["rev-parse", "--verify", "HEAD"]).is_err());
    assert!(!dest.join("file").exists());
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
    assert!(
        preview
            .verification_error
            .as_ref()
            .is_some_and(|e| e.contains(&base)),
        "{:?}",
        preview.verification_error
    );
    assert_eq!(preview.prerequisites[0].oid, base);
    assert_eq!(run(Some(&dest), &["count-objects", "-v"])?, object_store);
    assert_eq!(refs(&dest)?, refs_before);
    assert!(!marker.exists());
    assert!(import_bundle(&dest, &preview, &[mapping("refs/heads/imported")]).is_err());
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
    // Positive control: this same prerequisite is fetchable, and ordinary native verify
    // really does contact our upload-pack helper and mutate this fixture's object store.
    run(
        Some(&dest),
        &["bundle", "verify", bundle.to_str().ok_or("bundle path")?],
    )?;
    assert!(marker.exists());
    assert_ne!(run(Some(&dest), &["count-objects", "-v"])?, object_store);
    let calls = std::fs::read(&marker).map_err(|e| e.to_string())?;
    import_bundle(&dest, &preview, &[mapping("refs/heads/imported")])?;
    assert_eq!(std::fs::read(&marker).map_err(|e| e.to_string())?, calls);
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
    let (args, input) = verification_command(&["--no-lazy-fetch"], &long_path, &bytes)?;
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
    let (args, input) = verification_command(&[], &long_path, &bytes)?;
    assert_eq!(args.last().copied(), Some(long_path.as_str()));
    assert!(input.is_empty());
    assert!(verification_command(&["--no-lazy-fetch"], &long_path, b"not a bundle").is_err());
    Ok(())
}
