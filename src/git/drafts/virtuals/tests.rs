use super::*;
pub(crate) struct Fixture(tempfile::TempDir);
impl Fixture {
    pub(crate) fn new() -> Result<Self, String> {
        let f = Self(tempfile::tempdir().map_err(|e| e.to_string())?);
        for args in [
            vec!["init", "-q", "-b", "main"],
            vec!["config", "user.name", "Fixture"],
            vec!["config", "user.email", "fixture@example.com"],
            vec!["config", "commit.gpgSign", "false"],
        ] {
            f.git(&args)?;
        }
        f.write("file", f.base().as_bytes())?;
        f.write("other", b"other base\n")?;
        f.git(&["add", "."])?;
        f.git(&["commit", "-qm", "base"])?;
        Ok(f)
    }
    pub(crate) fn base(&self) -> String {
        (1..=40).map(|i| format!("line {i}\n")).collect()
    }
    pub(crate) fn path(&self) -> &Path {
        self.0.path()
    }
    pub(crate) fn git(&self, args: &[&str]) -> Result<String, String> {
        git::run(Some(self.path()), args).map(|s| s.trim_end().into())
    }
    pub(crate) fn write(&self, p: &str, b: &[u8]) -> Result<(), String> {
        std::fs::write(self.path().join(p), b).map_err(|e| e.to_string())
    }
    pub(crate) fn read(&self) -> Result<View, String> {
        read(self.path(), &|| true)
    }
    pub(crate) fn edit(&self, e: Edit) -> Result<(), String> {
        edit(self.path(), &self.read()?, e, &|| true).map(|_| ())
    }
    pub(crate) fn own(&self, name: &str, fragment: &str) -> Result<String, String> {
        let v = self.read()?;
        let key = v
            .hunks
            .iter()
            .find(|h| h.patch.contains(fragment))
            .ok_or("No matching hunk")?
            .key
            .clone();
        self.edit(Edit::Assign(name.into(), key.clone()))?;
        Ok(key)
    }
}
#[test]
fn virtual_commits_isolate_two_hunks_in_one_file_without_moving_anchor_or_index()
-> Result<(), String> {
    let f = Fixture::new()?;
    let head = f.git(&["rev-parse", "HEAD"])?;
    let work = f
        .base()
        .replace("line 3\n", "ONE\n")
        .replace("line 30\n", "TWO\n");
    f.write("file", work.as_bytes())?;
    f.write("other", b"other staged\n")?;
    f.git(&["add", "other"])?;
    f.write("untracked", b"unowned untracked\n")?;
    let index = std::fs::read(f.path().join(".git/index")).map_err(|e| e.to_string())?;
    f.edit(Edit::Create("alpha".into()))?;
    f.edit(Edit::Create("beta".into()))?;
    f.own("alpha", "+ONE")?;
    f.own("beta", "+TWO")?;
    f.edit(Edit::Message("alpha".into(), "Only one".into()))?;
    f.edit(Edit::Message("beta".into(), "Only two".into()))?;
    let p = prepare_commit(f.path(), &f.read()?, "alpha", &|| true)?;
    assert!(p.patch.contains("+ONE"));
    assert!(!p.patch.contains("+TWO"));
    commit(f.path(), &p, &|| true)?;
    let p = prepare_commit(f.path(), &f.read()?, "beta", &|| true)?;
    commit(f.path(), &p, &|| true)?;
    assert_eq!(
        f.git(&["show", "alpha:file"])?,
        f.base().replace("line 3\n", "ONE\n").trim_end()
    );
    assert_eq!(
        f.git(&["show", "beta:file"])?,
        f.base().replace("line 30\n", "TWO\n").trim_end()
    );
    assert_eq!(f.git(&["rev-parse", "HEAD"])?, head);
    assert_eq!(f.git(&["symbolic-ref", "HEAD"])?, "refs/heads/main");
    assert_eq!(
        std::fs::read(f.path().join(".git/index")).map_err(|e| e.to_string())?,
        index
    );
    assert_eq!(
        std::fs::read(f.path().join("file")).map_err(|e| e.to_string())?,
        work.as_bytes()
    );
    assert_eq!(
        std::fs::read(f.path().join("untracked")).map_err(|e| e.to_string())?,
        b"unowned untracked\n"
    );
    let beta = f.git(&["rev-parse", "beta"])?;
    undo(f.path(), &f.read()?, &|| true)?;
    assert_eq!(f.git(&["rev-parse", "beta"])?, head);
    assert_ne!(beta, head);
    assert_eq!(
        std::fs::read(f.path().join(".git/index")).map_err(|e| e.to_string())?,
        index
    );
    Ok(())
}
#[test]
fn virtual_activation_retains_other_hunks_restores_new_file_permissions_and_rejects_overlap()
-> Result<(), String> {
    let f = Fixture::new()?;
    let work = f
        .base()
        .replace("line 3\n", "ONE\n")
        .replace("line 30\n", "UNOWNED\n");
    f.write("file", work.as_bytes())?;
    f.write("owned-new", b"new data\n")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            f.path().join("owned-new"),
            std::fs::Permissions::from_mode(0o640),
        )
        .map_err(|e| e.to_string())?;
    }
    f.edit(Edit::Create("alpha".into()))?;
    f.own("alpha", "+ONE")?;
    f.own("alpha", "+new data")?;
    let index = std::fs::read(f.path().join(".git/index")).map_err(|e| e.to_string())?;
    toggle(f.path(), &f.read()?, "alpha", &|| true)?;
    assert_eq!(
        std::fs::read(f.path().join("file")).map_err(|e| e.to_string())?,
        f.base().replace("line 30\n", "UNOWNED\n").as_bytes()
    );
    assert!(!f.path().join("owned-new").exists());
    toggle(f.path(), &f.read()?, "alpha", &|| true)?;
    assert_eq!(
        std::fs::read(f.path().join("file")).map_err(|e| e.to_string())?,
        work.as_bytes()
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(f.path().join("owned-new"))
                .map_err(|e| e.to_string())?
                .permissions()
                .mode()
                & 0o777,
            0o640
        );
    }
    undo(f.path(), &f.read()?, &|| true)?;
    assert!(!f.path().join("owned-new").exists());
    f.write(
        "file",
        f.base()
            .replace("line 3\n", "CONFLICT\n")
            .replace("line 30\n", "UNOWNED\n")
            .as_bytes(),
    )?;
    let before = std::fs::read(f.path().join("file")).map_err(|e| e.to_string())?;
    let version = f.read()?.version;
    assert!(toggle(f.path(), &f.read()?, "alpha", &|| true).is_err());
    assert_eq!(f.read()?.version, version);
    assert_eq!(
        std::fs::read(f.path().join("file")).map_err(|e| e.to_string())?,
        before
    );
    assert_eq!(
        std::fs::read(f.path().join(".git/index")).map_err(|e| e.to_string())?,
        index
    );
    Ok(())
}
#[test]
fn virtual_stale_ownership_and_external_branch_changes_refuse_writes() -> Result<(), String> {
    let f = Fixture::new()?;
    f.write("file", f.base().replace("line 3\n", "ONE\n").as_bytes())?;
    f.edit(Edit::Create("alpha".into()))?;
    let key = f.own("alpha", "+ONE")?;
    f.edit(Edit::Message("alpha".into(), "Feature".into()))?;
    f.write("file", f.base().replace("line 3\n", "CHANGED\n").as_bytes())?;
    assert!(prepare_commit(f.path(), &f.read()?, "alpha", &|| true).is_err());
    assert!(toggle(f.path(), &f.read()?, "alpha", &|| true).is_err());
    f.edit(Edit::Release("alpha".into(), key))?;
    f.own("alpha", "+CHANGED")?;
    let p = prepare_commit(f.path(), &f.read()?, "alpha", &|| true)?;
    let tree = f.git(&["rev-parse", "HEAD^{tree}"])?;
    let old = f.git(&["rev-parse", "alpha"])?;
    let new = f.git(&["commit-tree", &tree, "-p", &old, "-m", "External"])?;
    f.git(&["update-ref", "refs/heads/alpha", &new, &old])?;
    let before = std::fs::read(f.path().join("file")).map_err(|e| e.to_string())?;
    assert!(commit(f.path(), &p, &|| true).is_err());
    assert_eq!(f.git(&["rev-parse", "alpha"])?, new);
    assert_eq!(
        std::fs::read(f.path().join("file")).map_err(|e| e.to_string())?,
        before
    );
    Ok(())
}
#[test]
fn virtual_checked_out_branch_and_overlapping_staging_protect_both_worktrees() -> Result<(), String>
{
    let f = Fixture::new()?;
    f.write("file", f.base().replace("line 3\n", "ONE\n").as_bytes())?;
    f.edit(Edit::Create("alpha".into()))?;
    f.own("alpha", "+ONE")?;
    f.edit(Edit::Message("alpha".into(), "Feature".into()))?;
    let p = prepare_commit(f.path(), &f.read()?, "alpha", &|| true)?;
    let host = tempfile::tempdir().map_err(|e| e.to_string())?;
    let other = host.path().join("owned-worktree");
    f.git(&[
        "worktree",
        "add",
        other.to_str().ok_or("Bad path")?,
        "alpha",
    ])?;
    assert!(commit(f.path(), &p, &|| true).is_err());
    assert_eq!(
        git::run(Some(&other), &["rev-parse", "HEAD"])?.trim(),
        f.git(&["rev-parse", "HEAD"])?
    );
    assert_eq!(
        std::fs::read(other.join("file")).map_err(|e| e.to_string())?,
        f.base().as_bytes()
    );
    f.git(&["add", "file"])?;
    let before = std::fs::read(f.path().join("file")).map_err(|e| e.to_string())?;
    assert!(toggle(f.path(), &f.read()?, "alpha", &|| true).is_err());
    assert_eq!(
        std::fs::read(f.path().join("file")).map_err(|e| e.to_string())?,
        before
    );
    Ok(())
}
