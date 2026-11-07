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
