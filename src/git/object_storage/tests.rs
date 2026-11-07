use super::*;
#[test]
fn verify_pack_parser_keeps_actual_delta_numbers() -> Result<(), String> {
    let a = "a".repeat(40);
    let b = "b".repeat(40);
    let rows = pack_rows(&format!(
        "{a} blob 9000 22 12 1 {b}\n{b} blob 8990 38 34\nnon delta: 1 object\n"
    ))?;
    assert_eq!(rows[&a].disk, 22);
    assert_eq!(rows[&a].base.as_ref(), Some(&b));
    assert_eq!(rows[&a].depth, 1);
    assert_eq!(rows[&b].offset, 34);
    assert!(pack_rows(&format!("{a} blob 2 3 4 1 bad\n")).is_err());
    Ok(())
}

use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Result<Self, String> {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "gitcraft-objects-{}-{} quoted' $(touch ESCAPED)",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).map_err(|e| e.to_string())?;
        Ok(Self(path))
    }
    fn repo(&self, name: &str, format: &str) -> Result<PathBuf, String> {
        let path = self.0.join(name);
        std::fs::create_dir(&path).map_err(|e| e.to_string())?;
        Self::git(
            &path,
            &[
                "init",
                "-q",
                "--template=",
                "-b",
                "main",
                &format!("--object-format={format}"),
            ],
        )?;
        for (k, v) in [
            ("user.name", "Fixture"),
            ("user.email", "fixture@example.invalid"),
            ("commit.gpgSign", "false"),
            ("core.autocrlf", "false"),
            ("core.hooksPath", "/nonexistent/gitcraft-fixture-hooks"),
        ] {
            Self::git(&path, &["config", k, v])?;
        }
        Ok(path)
    }
    fn git(repo: &Path, args: &[&str]) -> Result<String, String> {
        let mut cmd = Command::new("git");
        crate::shell_path::apply(&mut cmd);
        cmd.arg("-C")
            .arg(repo)
            .args(args)
            .env("GIT_CONFIG_NOSYSTEM", "1");
        for key in [
            "GIT_DIR",
            "GIT_COMMON_DIR",
            "GIT_WORK_TREE",
            "GIT_OBJECT_DIRECTORY",
            "GIT_ALTERNATE_OBJECT_DIRECTORIES",
            "GIT_INDEX_FILE",
            "GIT_CONFIG_COUNT",
            "GIT_CONFIG_PARAMETERS",
        ] {
            cmd.env_remove(key);
        }
        let out = cmd.output().map_err(|e| e.to_string())?;
        if !out.status.success() {
            return Err(format!(
                "Git fixture: {}",
                String::from_utf8_lossy(&out.stderr)
            ));
        }
        Ok(String::from_utf8_lossy(&out.stdout)
            .trim_end_matches(['\n', '\r'])
            .into())
    }
    fn commit(repo: &Path, content: &[u8]) -> Result<String, String> {
        std::fs::write(repo.join("file ' $(touch ESCAPED).txt"), content)
            .map_err(|e| e.to_string())?;
        Self::git(repo, &["add", "-A"])?;
        Self::git(repo, &["commit", "-qm", "fixture"])?;
        Self::git(repo, &["rev-parse", "HEAD"])
    }
    fn borrowing(&self) -> Result<(PathBuf, PathBuf), String> {
        let donor = self.repo("donor", "sha1")?;
        Self::commit(&donor, b"borrowed bytes\n")?;
        let borrower = self.0.join("borrower");
        Self::git(
            &self.0,
            &[
                "clone",
                "--shared",
                "--no-hardlinks",
                "--",
                donor.to_str().ok_or("non UTF8")?,
                borrower.to_str().ok_or("non UTF8")?,
            ],
        )?;
        Ok((donor, borrower))
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn actual_loose_file_versions_trees_missing_and_sha256() -> Result<(), String> {
    let f = Fixture::new()?;
    for format in ["sha1", "sha256"] {
        let repo = f.repo(format, format)?;
        let head = Fixture::commit(&repo, b"file contents\n")?;
        let query = "HEAD:file ' $(touch ESCAPED).txt";
        let got = inspect(&repo, query, &|| true)?;
        let object = got.object.ok_or("missing object")?;
        assert_eq!(object.kind, "blob");
        assert_eq!(object.size, 14);
        assert_eq!(got.format, format);
        assert!(
            got.representations
                .iter()
                .any(|r| r.loose_bytes.is_some_and(|n| n > 0))
        );
        assert_eq!(object.oid.len(), if format == "sha1" { 40 } else { 64 });
        let commit = inspect(&repo, &head, &|| true)?;
        let tree = commit
            .entries
            .iter()
            .find(|e| e.name == "tree")
            .ok_or("tree link absent")?;
        let tree = inspect(&repo, &tree.oid, &|| true)?;
        assert!(
            tree.entries
                .iter()
                .any(|e| e.oid == object.oid && e.name == "file ' $(touch ESCAPED).txt")
        );
        let absent = inspect(&repo, &"f".repeat(object.oid.len()), &|| true)?;
        assert!(absent.object.is_none());
        assert!(absent.missing.is_some());
        assert!(inspect(&repo, "--output=ESCAPED", &|| true).is_err());
        assert!(inspect(&repo, "HEAD\nHEAD", &|| true).is_err());
        assert!(inspect(&repo, "HEAD", &|| false).is_err());
        assert!(!repo.join("ESCAPED").exists());
    }
    Ok(())
}
#[test]
fn actual_pack_representation_exposes_delta_ancestry() -> Result<(), String> {
    let f = Fixture::new()?;
    let repo = f.repo("packed", "sha1")?;
    for n in 0..12 {
        let mut data = (0..6000)
            .map(|i| format!("same line {i:06}\n"))
            .collect::<String>();
        data.push_str(&format!("version {n}\n"));
        Fixture::commit(&repo, data.as_bytes())?;
    }
    Fixture::git(&repo, &["repack", "-adf", "--window=50", "--depth=50"])?;
    let got = inspect(&repo, "HEAD~10:file ' $(touch ESCAPED).txt", &|| true)?;
    assert!(!got.representations.is_empty());
    let packed = got
        .representations
        .iter()
        .find(|r| r.loose_bytes.is_none())
        .ok_or("no packed representation")?;
    assert!(packed.path.extension().is_some_and(|s| s == "pack"));
    assert!(packed.chain.len() > 1, "expected real delta fixture");
    assert!(packed.chain[0].depth > 0);
    assert!(packed.chain.last().is_some_and(|row| row.base.is_none()));
    assert_eq!(packed.chain[0].base.as_ref(), Some(&packed.chain[1].oid));
    assert!(packed.chain[0].disk < got.object.as_ref().ok_or("missing object")?.size);
    Ok(())
}
#[cfg(target_os = "linux")]
#[test]
fn shared_clone_independence_survives_donor_removal() -> Result<(), String> {
    let f = Fixture::new()?;
    let (donor, borrower) = f.borrowing()?;
    let original = inspect(&borrower, "HEAD:file ' $(touch ESCAPED).txt", &|| true)?;
    assert_eq!(original.topology.stores.len(), 2);
    assert!(
        original
            .representations
            .iter()
            .any(|r| r.path.starts_with(donor.join(".git/objects")))
    );
    let donor_before = Fixture::git(&donor, &["count-objects", "-v"])?;
    let plan = original.independence.ok_or("no independence plan")?;
    let result = independent::make_independent(&plan, &|| true)?;
    assert!(result.contains("independent"));
    assert!(!borrower.join(".git/objects/info/alternates").exists());
    assert_eq!(
        Fixture::git(&donor, &["count-objects", "-v"])?,
        donor_before
    );
    std::fs::rename(&donor, f.0.join("removed donor")).map_err(|e| e.to_string())?;
    assert_eq!(
        Fixture::git(&borrower, &["show", "HEAD:file ' $(touch ESCAPED).txt"])?,
        "borrowed bytes"
    );
    Fixture::git(&borrower, &["fsck", "--full", "--no-dangling"])?;
    assert!(
        std::fs::read_dir(borrower.join(".git/objects/info"))
            .map_err(|e| e.to_string())?
            .filter_map(Result::ok)
            .any(|e| e
                .file_name()
                .to_string_lossy()
                .starts_with("alternates.gitcraft-backup-"))
    );
    Ok(())
}
#[cfg(target_os = "linux")]
#[test]
fn missing_donor_is_visible_and_copy_failure_preserves_borrowing() -> Result<(), String> {
    let f = Fixture::new()?;
    let (donor, borrower) = f.borrowing()?;
    let got = inspect(&borrower, "HEAD:file ' $(touch ESCAPED).txt", &|| true)?;
    let oid = got.object.ok_or("no blob")?.oid;
    let plan = got.independence.ok_or("no plan")?;
    let alternate = borrower.join(".git/objects/info/alternates");
    let before = std::fs::read(&alternate).map_err(|e| e.to_string())?;
    std::fs::remove_file(donor.join(".git/objects").join(&oid[..2]).join(&oid[2..]))
        .map_err(|e| e.to_string())?;
    assert!(independent::make_independent(&plan, &|| true).is_err());
    assert_eq!(
        std::fs::read(&alternate).map_err(|e| e.to_string())?,
        before
    );
    std::fs::rename(&donor, f.0.join("removed")).map_err(|e| e.to_string())?;
    let missing = inspect(&borrower, &oid, &|| true)?;
    assert!(missing.object.is_none());
    assert!(
        missing
            .topology
            .stores
            .iter()
            .any(|s| !s.available && s.donor_of.is_some())
    );
    assert!(missing.independence.is_none());
    Ok(())
}
#[cfg(target_os = "linux")]
#[test]
fn stale_alternates_refs_index_and_post_copy_changes_refuse_safely() -> Result<(), String> {
    let f = Fixture::new()?;
    let (_donor, borrower) = f.borrowing()?;
    let plan = inspect(&borrower, "HEAD", &|| true)?
        .independence
        .ok_or("no plan")?;
    Fixture::git(&borrower, &["update-ref", "refs/heads/new", "HEAD"])?;
    assert!(independent::make_independent(&plan, &|| true).is_err_and(|e| e.contains("changed")));
    let plan = inspect(&borrower, "HEAD", &|| true)?
        .independence
        .ok_or("no plan")?;
    std::fs::write(borrower.join("newfile"), b"staged object").map_err(|e| e.to_string())?;
    Fixture::git(&borrower, &["add", "newfile"])?;
    assert!(independent::make_independent(&plan, &|| true).is_err_and(|e| e.contains("changed")));
    let plan = inspect(&borrower, "HEAD", &|| true)?
        .independence
        .ok_or("no plan")?;
    let alternate = borrower.join(".git/objects/info/alternates");
    let before = std::fs::read(&alternate).map_err(|e| e.to_string())?;
    assert!(
        independent::make_with_checkpoints(&plan, &|| true, &mut |phase| {
            if phase == "copied" {
                Fixture::git(&borrower, &["update-ref", "refs/heads/race", "HEAD"])?;
            }
            Ok(())
        })
        .is_err_and(|e| e.contains("changed"))
    );
    assert_eq!(
        std::fs::read(&alternate).map_err(|e| e.to_string())?,
        before
    );
    let plan = inspect(&borrower, "HEAD", &|| true)?
        .independence
        .ok_or("no plan")?;
    std::fs::write(&alternate, b"/missing/replaced-donor\n").map_err(|e| e.to_string())?;
    assert!(independent::make_independent(&plan, &|| true).is_err());
    assert_eq!(
        std::fs::read(&alternate).map_err(|e| e.to_string())?,
        b"/missing/replaced-donor\n"
    );
    Ok(())
}
#[cfg(target_os = "linux")]
#[test]
fn failed_final_proof_restores_original_borrowing_link() -> Result<(), String> {
    let f = Fixture::new()?;
    let (_donor, borrower) = f.borrowing()?;
    let plan = inspect(&borrower, "HEAD", &|| true)?
        .independence
        .ok_or("no plan")?;
    let alternate = borrower.join(".git/objects/info/alternates");
    let before = std::fs::read(&alternate).map_err(|e| e.to_string())?;
    let result = independent::make_with_checkpoints(&plan, &|| true, &mut |phase| {
        if phase == "detached" {
            assert!(!alternate.exists());
            let packs = files::packs(&borrower.join(".git/objects"), 64, &|| true)?;
            for index in packs {
                std::fs::remove_file(index.with_extension("pack")).map_err(|e| e.to_string())?;
                std::fs::remove_file(index).map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    });
    assert!(result.is_err_and(|e| e.contains("Borrowing was restored")));
    assert_eq!(
        std::fs::read(&alternate).map_err(|e| e.to_string())?,
        before
    );
    assert_eq!(
        Fixture::git(&borrower, &["show", "HEAD:file ' $(touch ESCAPED).txt"])?,
        "borrowed bytes"
    );
    Ok(())
}
#[test]
fn promisor_and_linked_worktrees_disable_independence() -> Result<(), String> {
    let f = Fixture::new()?;
    let (_donor, borrower) = f.borrowing()?;
    Fixture::git(&borrower, &["config", "remote.origin.promisor", "true"])?;
    let data = inspect(&borrower, "HEAD", &|| true)?;
    assert!(data.topology.promisor);
    assert!(data.independence.is_none());
    Fixture::git(&borrower, &["config", "--unset", "remote.origin.promisor"])?;
    let linked = f.0.join("linked");
    Fixture::git(
        &borrower,
        &[
            "worktree",
            "add",
            "--detach",
            linked.to_str().ok_or("path")?,
            "HEAD",
        ],
    )?;
    let data = inspect(&linked, "HEAD", &|| true)?;
    assert_ne!(data.git_dir, data.common_dir);
    assert!(data.independence.is_none());
    assert!(data.topology.worktrees.contains("linked"));
    Ok(())
}

#[test]
fn known_promised_blobs_are_distinguished_without_fetching() -> Result<(), String> {
    let f = Fixture::new()?;
    let donor = f.repo("partial-source", "sha1")?;
    Fixture::commit(&donor, b"not yet downloaded\n")?;
    Fixture::git(&donor, &["config", "uploadpack.allowFilter", "true"])?;
    let oid = Fixture::git(&donor, &["rev-parse", "HEAD:file ' $(touch ESCAPED).txt"])?;
    let partial = f.0.join("partial");
    Fixture::git(
        &f.0,
        &[
            "clone",
            "--filter=blob:none",
            "--no-checkout",
            "--no-local",
            "--",
            donor.to_str().ok_or("path")?,
            partial.to_str().ok_or("path")?,
        ],
    )?;
    Fixture::git(
        &partial,
        &[
            "remote",
            "set-url",
            "origin",
            "/nonexistent/gitcraft-never-fetch",
        ],
    )?;
    let before = files::packs(&partial.join(".git/objects"), 64, &|| true)?;
    let data = inspect(&partial, &oid, &|| true)?;
    assert!(data.object.is_none());
    assert!(
        data.missing
            .as_ref()
            .is_some_and(|s| s.contains("missing promised object"))
    );
    assert_eq!(
        files::packs(&partial.join(".git/objects"), 64, &|| true)?,
        before
    );
    assert!(data.independence.is_none());
    Ok(())
}

#[test]
fn fallible_profile_is_preserved_and_practice_bypasses_it() -> Result<(), String> {
    let f = Fixture::new()?;
    let repo = f.repo("profile", "sha1")?;
    Fixture::commit(&repo, b"normal\n")?;
    let broken = crate::connections::Context {
        error: Some("Profile cannot be materialized".into()),
        ..Default::default()
    };
    broken.run(|| {
        assert!(
            inspect(&repo, "HEAD", &|| true)
                .is_err_and(|e| e.contains("Profile cannot be materialized"))
        );
    });
    let practice = crate::practice::create(crate::practice::Scenario::Published)?;
    let result = broken.run(|| inspect(&practice, "HEAD", &|| true));
    if let Some(root) = practice.parent() {
        let _ = std::fs::remove_dir_all(root);
    }
    assert!(result?.object.is_some());
    Ok(())
}

#[cfg(unix)]
#[test]
fn fake_ssh_inspection_and_independence_pin_paths_with_hostile_environment() -> Result<(), String> {
    const CHILD: &str = "GITCRAFT_OBJECT_STORAGE_SSH_FIXTURE";
    const NAME: &str = "git::object_storage::tests::fake_ssh_inspection_and_independence_pin_paths_with_hostile_environment";
    if let Some(path) = std::env::var_os(CHILD) {
        let path = PathBuf::from(path);
        // The local route must reject inherited GIT_DIR/GIT_OBJECT_DIRECTORY as well.
        let local = inspect(&path, "HEAD:file ' $(touch ESCAPED).txt", &|| true)?;
        assert_eq!(local.object.as_ref().ok_or("no local object")?.size, 15);
        let remote = remote::encode(
            &remote::Host::Ssh("objects-fixture.invalid".into()),
            path.to_str().ok_or("path")?,
        );
        let data = inspect(&remote, "HEAD:file ' $(touch ESCAPED).txt", &|| true)?;
        assert_eq!(data.topology.stores.len(), 2);
        assert_eq!(data.object.as_ref().ok_or("no remote object")?.size, 15);
        assert!(data.independence.is_none());
        assert!(
            data.notes
                .iter()
                .any(|s| s.contains("Read-only inspection remains"))
        );
        assert!(path.join(".git/objects/info/alternates").exists());
        assert!(!path.join("ESCAPED").exists());
        return Ok(());
    }
    let f = Fixture::new()?;
    let (_donor, borrower) = f.borrowing()?;
    let hostile = f.repo("hostile", "sha1")?;
    Fixture::commit(&hostile, b"incorrect repository contents\n")?;
    let bin = f.0.join("fixture-bin");
    std::fs::create_dir(&bin).map_err(|e| e.to_string())?;
    let ssh = bin.join("ssh");
    std::fs::write(&ssh, "#!/bin/sh\nexec /bin/sh\n").map_err(|e| e.to_string())?;
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&ssh, std::fs::Permissions::from_mode(0o700))
        .map_err(|e| e.to_string())?;
    let mut paths = vec![bin];
    paths.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    let output = Command::new(std::env::current_exe().map_err(|e| e.to_string())?)
        .args(["--exact", NAME, "--nocapture"])
        .env(CHILD, &borrower)
        .env(
            "PATH",
            std::env::join_paths(paths).map_err(|e| e.to_string())?,
        )
        .env("GIT_DIR", hostile.join(".git"))
        .env("GIT_WORK_TREE", &hostile)
        .env("GIT_OBJECT_DIRECTORY", hostile.join(".git/objects"))
        .env("GIT_ALTERNATE_OBJECT_DIRECTORIES", "/missing/ambient-donor")
        .output()
        .map_err(|e| e.to_string())?;
    assert!(
        output.status.success(),
        "{} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(borrower.join(".git/objects/info/alternates").exists());
    assert_eq!(
        Fixture::git(&borrower, &["show", "HEAD:file ' $(touch ESCAPED).txt"])?,
        "borrowed bytes"
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn fake_wsl_uses_production_bounded_object_request_without_a_live_distribution()
-> Result<(), String> {
    const CHILD: &str = "GITCRAFT_OBJECT_STORAGE_WSL_FIXTURE";
    const NAME: &str = "git::object_storage::tests::fake_wsl_uses_production_bounded_object_request_without_a_live_distribution";
    if let Some(path) = std::env::var_os(CHILD) {
        let path = PathBuf::from(path);
        let target = remote::Target {
            host: remote::Host::Wsl("ObjectsFixture".into()),
            path: path.to_str().ok_or("path")?.into(),
        };
        let oid = Fixture::git(&path, &["rev-parse", "HEAD:file ' $(touch ESCAPED).txt"])?;
        let request = super::super::command::object_storage_request(
            &target,
            &[
                "cat-file",
                "--batch-check=%(objectname) %(objecttype) %(objectsize)",
            ],
            format!("{oid}\n").into_bytes(),
            4096,
        );
        assert!(request.script.contains("--no-lazy-fetch"));
        assert!(request.unset.iter().any(|s| s == "GIT_DIR"));
        assert!(
            request
                .unset
                .iter()
                .any(|s| s == "GIT_ALTERNATE_OBJECT_DIRECTORIES")
        );
        let out = remote::exec(&target.host, &request)?;
        assert_eq!(out.code, 0, "{}", String::from_utf8_lossy(&out.stderr));
        assert_eq!(
            String::from_utf8_lossy(&out.stdout).trim(),
            format!("{oid} blob 15")
        );
        let request = super::super::command::object_storage_request(
            &target,
            &["cat-file", "--batch-check=%(objectname) %(objecttype)"],
            format!("{}\n", "f".repeat(40)).into_bytes(),
            4096,
        );
        let out = remote::exec(&target.host, &request)?;
        assert_eq!(out.code, 0);
        assert!(String::from_utf8_lossy(&out.stdout).contains("missing"));
        assert!(!path.join("ESCAPED").exists());
        return Ok(());
    }
    let f = Fixture::new()?;
    let (_donor, borrower) = f.borrowing()?;
    let bin = f.0.join("fixture-bin");
    std::fs::create_dir(&bin).map_err(|e| e.to_string())?;
    let wsl = bin.join("wsl.exe");
    std::fs::write(&wsl, "#!/bin/sh\nexec /bin/sh\n").map_err(|e| e.to_string())?;
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&wsl, std::fs::Permissions::from_mode(0o700))
        .map_err(|e| e.to_string())?;
    let mut paths = vec![bin];
    paths.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    let output = Command::new(std::env::current_exe().map_err(|e| e.to_string())?)
        .args(["--exact", NAME, "--nocapture"])
        .env(CHILD, borrower)
        .env(
            "PATH",
            std::env::join_paths(paths).map_err(|e| e.to_string())?,
        )
        .env(
            "GIT_ALTERNATE_OBJECT_DIRECTORIES",
            "/nonexistent/ambient-donor",
        )
        .output()
        .map_err(|e| e.to_string())?;
    assert!(
        output.status.success(),
        "{} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

#[cfg(target_os = "linux")]
mod fixed_ownership {
    use super::*;
    fn copy_owned_tree(from: &Path, to: &Path) -> Result<(), String> {
        std::fs::create_dir(to).map_err(|e| e.to_string())?;
        for entry in std::fs::read_dir(from).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let target = to.join(entry.file_name());
            if entry.file_type().map_err(|e| e.to_string())?.is_dir() {
                copy_owned_tree(&entry.path(), &target)?;
            } else {
                std::fs::copy(entry.path(), target).map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    }
    #[test]
    fn fixed_candidate_refuses_git_directory_and_repository_ancestor_links() -> Result<(), String> {
        let f = Fixture::new()?;
        let (_, borrower) = f.borrowing()?;
        let git = borrower.join(".git");
        let saved = borrower.join("actual-git");
        std::fs::rename(&git, &saved).map_err(|e| e.to_string())?;
        std::os::unix::fs::symlink(&saved, &git).map_err(|e| e.to_string())?;
        let data = inspect(&borrower, "HEAD", &|| true)?;
        assert!(data.object.is_some());
        assert!(data.independence.is_none());
        std::fs::remove_file(&git).map_err(|e| e.to_string())?;
        std::fs::rename(&saved, &git).map_err(|e| e.to_string())?;
        let alias = f.0.join("ancestor-alias");
        std::os::unix::fs::symlink(&borrower, &alias).map_err(|e| e.to_string())?;
        assert!(inspect(&alias, "HEAD", &|| true)?.independence.is_none());
        assert!(git.join("objects/info/alternates").is_file());
        Ok(())
    }
    #[test]
    fn fixed_candidate_rejects_same_text_git_directory_replacement_before_action()
    -> Result<(), String> {
        let f = Fixture::new()?;
        let (_, borrower) = f.borrowing()?;
        let plan = inspect(&borrower, "HEAD", &|| true)?
            .independence
            .ok_or("no plan")?;
        let git = borrower.join(".git");
        let saved = borrower.join("held-git");
        std::fs::rename(&git, &saved).map_err(|e| e.to_string())?;
        copy_owned_tree(&saved, &git)?;
        assert!(
            independent::make_independent(&plan, &|| true)
                .is_err_and(|e| e.contains("identity changed"))
        );
        assert!(git.join("objects/info/alternates").is_file());
        assert!(saved.join("objects/info/alternates").is_file());
        Ok(())
    }
    #[test]
    fn fixed_candidate_holds_git_info_and_ancestor_identity_after_final_verification()
    -> Result<(), String> {
        for part in ["git", "info", "ancestor", "alternate"] {
            let f = Fixture::new()?;
            let (_, borrower) = f.borrowing()?;
            let plan = inspect(&borrower, "HEAD", &|| true)?
                .independence
                .ok_or("no plan")?;
            let alternate = borrower.join(".git/objects/info/alternates");
            let expected = std::fs::read(&alternate).map_err(|e| e.to_string())?;
            let (current, moved) = match part {
                "git" => (borrower.join(".git"), borrower.join("held-git")),
                "info" => (
                    borrower.join(".git/objects/info"),
                    borrower.join(".git/objects/held-info"),
                ),
                "ancestor" => (borrower.clone(), f.0.join("held-borrower")),
                _ => (
                    alternate.clone(),
                    borrower.join(".git/objects/info/held-alternate"),
                ),
            };
            let result = independent::make_with_checkpoints(&plan, &|| true, &mut |phase| {
                if phase == "verified" {
                    std::fs::rename(&current, &moved).map_err(|e| e.to_string())?;
                    if part == "alternate" {
                        std::fs::write(&current, &expected).map_err(|e| e.to_string())?;
                    } else {
                        copy_owned_tree(&moved, &current)?;
                    }
                }
                Ok(())
            });
            assert!(result.is_err_and(|e| e.contains("identity")), "{part}");
            assert_eq!(
                std::fs::read(&alternate).map_err(|e| e.to_string())?,
                expected
            );
            let retained = match part {
                "git" => moved.join("objects/info/alternates"),
                "info" => moved.join("alternates"),
                "ancestor" => moved.join(".git/objects/info/alternates"),
                _ => moved,
            };
            assert_eq!(
                std::fs::read(retained).map_err(|e| e.to_string())?,
                expected
            );
        }
        Ok(())
    }
    #[test]
    fn fixed_candidate_restoration_uses_held_info_and_does_not_overwrite_new_metadata()
    -> Result<(), String> {
        let f = Fixture::new()?;
        let (_, borrower) = f.borrowing()?;
        let plan = inspect(&borrower, "HEAD", &|| true)?
            .independence
            .ok_or("no plan")?;
        let info = borrower.join(".git/objects/info");
        let held = borrower.join(".git/objects/held-info");
        let expected = std::fs::read(info.join("alternates")).map_err(|e| e.to_string())?;
        let result = independent::make_with_checkpoints(&plan, &|| true, &mut |phase| {
            if phase == "detached" {
                std::fs::rename(&info, &held).map_err(|e| e.to_string())?;
                std::fs::create_dir(&info).map_err(|e| e.to_string())?;
                std::fs::write(info.join("alternates"), b"new metadata must survive\n")
                    .map_err(|e| e.to_string())?;
            }
            Ok(())
        });
        assert!(result.is_err_and(|e| e.contains("Borrowing was restored")));
        assert_eq!(
            std::fs::read(info.join("alternates")).map_err(|e| e.to_string())?,
            b"new metadata must survive\n"
        );
        assert_eq!(
            std::fs::read(held.join("alternates")).map_err(|e| e.to_string())?,
            expected
        );
        Ok(())
    }
    #[test]
    fn fixed_candidate_create_only_recovery_preserves_a_new_alternate_file() -> Result<(), String> {
        let f = Fixture::new()?;
        let (_, borrower) = f.borrowing()?;
        let plan = inspect(&borrower, "HEAD", &|| true)?
            .independence
            .ok_or("no plan")?;
        let info = borrower.join(".git/objects/info");
        let expected = std::fs::read(info.join("alternates")).map_err(|e| e.to_string())?;
        let result = independent::make_with_checkpoints(&plan, &|| true, &mut |phase| {
            if phase == "detached" {
                std::fs::write(info.join("alternates"), b"new alternate\n")
                    .map_err(|e| e.to_string())?;
                return Err("owned fixture interrupts final proof".into());
            }
            Ok(())
        });
        assert!(result.is_err_and(|e| e.contains("did not overwrite")));
        assert_eq!(
            std::fs::read(info.join("alternates")).map_err(|e| e.to_string())?,
            b"new alternate\n"
        );
        let backups = std::fs::read_dir(&info)
            .map_err(|e| e.to_string())?
            .filter_map(Result::ok)
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("alternates.gitcraft-backup-")
            })
            .collect::<Vec<_>>();
        assert_eq!(backups.len(), 1);
        assert_eq!(
            std::fs::read(backups[0].path()).map_err(|e| e.to_string())?,
            expected
        );
        Ok(())
    }
}

#[cfg(windows)]
mod native_windows_ownership {
    use super::*;
    #[test]
    fn native_held_ancestors_and_alternate_exclude_replacement_and_writers() -> Result<(), String> {
        let f = Fixture::new()?;
        let (_, borrower) = f.borrowing()?;
        let plan = inspect(&borrower, "HEAD", &|| true)?
            .independence
            .ok_or("no native plan")?;
        let alt = borrower.join(".git/objects/info/alternates");
        assert!(std::fs::rename(borrower.join(".git"), borrower.join("replacement-git")).is_err());
        assert!(std::fs::rename(&borrower, f.0.join("replacement-root")).is_err());
        assert!(std::fs::remove_file(&alt).is_err());
        assert!(std::fs::OpenOptions::new().write(true).open(&alt).is_err());
        drop(plan);
        Ok(())
    }
    #[test]
    fn native_independence_clears_only_the_exclusively_held_alternate() -> Result<(), String> {
        let f = Fixture::new()?;
        let (donor, borrower) = f.borrowing()?;
        let plan = inspect(&borrower, "HEAD", &|| true)?
            .independence
            .ok_or("no native plan")?;
        independent::make_independent(&plan, &|| true)?;
        assert!(
            std::fs::read(borrower.join(".git/objects/info/alternates"))
                .map_err(|e| e.to_string())?
                .is_empty()
        );
        std::fs::rename(&donor, f.0.join("removed donor")).map_err(|e| e.to_string())?;
        assert_eq!(
            Fixture::git(&borrower, &["show", "HEAD:file ' $(touch ESCAPED).txt"])?,
            "borrowed bytes"
        );
        Fixture::git(&borrower, &["fsck", "--full", "--no-dangling"])?;
        drop(plan);
        Ok(())
    }
    #[test]
    fn native_failed_proof_recovers_the_same_exclusively_owned_file() -> Result<(), String> {
        let f = Fixture::new()?;
        let (_, borrower) = f.borrowing()?;
        let plan = inspect(&borrower, "HEAD", &|| true)?
            .independence
            .ok_or("no native plan")?;
        let alt = borrower.join(".git/objects/info/alternates");
        let before = std::fs::read(&alt).map_err(|e| e.to_string())?;
        let result = independent::make_with_checkpoints(&plan, &|| true, &mut |phase| {
            if phase == "detached" {
                assert!(std::fs::read(&alt).map_err(|e| e.to_string())?.is_empty());
                assert!(std::fs::remove_file(&alt).is_err());
                return Err("owned native fixture stops final proof".into());
            }
            Ok(())
        });
        assert!(result.is_err_and(|e| e.contains("Borrowing was restored")));
        assert_eq!(std::fs::read(&alt).map_err(|e| e.to_string())?, before);
        drop(plan);
        Ok(())
    }
}
