use super::*;
struct Fixture(tempfile::TempDir);
impl Fixture {
    fn new() -> Result<Self, String> {
        let f = Self(tempfile::tempdir().map_err(|e| e.to_string())?);
        for args in [
            vec!["init", "-q", "-b", "main"],
            vec!["config", "user.name", "Fixture"],
            vec!["config", "user.email", "fixture@example.com"],
            vec!["config", "commit.gpgSign", "false"],
        ] {
            f.git(&args)?;
        }
        f.write("a", b"a base\n")?;
        f.write("b", b"b base\n")?;
        f.git(&["add", "."])?;
        f.git(&["commit", "-qm", "base"])?;
        Ok(f)
    }
    fn path(&self) -> &Path {
        self.0.path()
    }
    fn git(&self, a: &[&str]) -> Result<String, String> {
        git::run(Some(self.path()), a).map(|s| s.trim_end().into())
    }
    fn write(&self, p: &str, b: &[u8]) -> Result<(), String> {
        std::fs::write(self.path().join(p), b).map_err(|e| e.to_string())
    }
    fn read(&self) -> Result<View, String> {
        read(self.path(), &|| true)
    }
    fn create(&self, name: &str, paths: &[&str]) -> Result<String, String> {
        edit(
            self.path(),
            &self.read()?,
            Edit::Create(name.into(), format!("Commit {name}")),
            &|| true,
        )?;
        let id = self.read()?.lists.lists.last().ok_or("No list")?.id.clone();
        for path in paths {
            edit(
                self.path(),
                &self.read()?,
                Edit::Assign(id.clone(), (*path).into()),
                &|| true,
            )?;
        }
        Ok(id)
    }
}
#[test]
fn changelist_commit_is_exact_and_keeps_other_staged_work_and_untracked_files() -> Result<(), String>
{
    let f = Fixture::new()?;
    f.write("a", b"a staged\n")?;
    f.git(&["add", "a"])?;
    f.write("a", b"a reviewed working\n")?;
    f.write("b", b"b staged\n")?;
    f.git(&["add", "b"])?;
    f.write("b", b"b later work\n")?;
    f.write("untracked", b"keep this\n")?;
    let id = f.create("Feature", &["a"])?;
    let other = f.create("Cleanup", &["b"])?;
    let b = f.git(&["rev-parse", ":b"])?;
    let head = f.git(&["rev-parse", "HEAD"])?;
    commit(f.path(), &f.read()?, &id, &|| true)?;
    assert_ne!(f.git(&["rev-parse", "HEAD"])?, head);
    assert_eq!(f.git(&["show", "HEAD:a"])?, "a reviewed working");
    assert_eq!(f.git(&["show", "HEAD:b"])?, "b base");
    assert_eq!(f.git(&["show", ":b"])?, "b staged");
    assert_eq!(f.git(&["rev-parse", ":b"])?, b);
    assert_eq!(
        std::fs::read(f.path().join("b")).map_err(|e| e.to_string())?,
        b"b later work\n"
    );
    assert_eq!(
        std::fs::read(f.path().join("untracked")).map_err(|e| e.to_string())?,
        b"keep this\n"
    );
    assert!(
        f.read()?
            .lists
            .lists
            .iter()
            .find(|l| l.id == id)
            .ok_or("No list")?
            .paths
            .is_empty()
    );
    assert_eq!(
        f.read()?
            .lists
            .lists
            .iter()
            .find(|l| l.id == other)
            .ok_or("No list")?
            .message,
        "Commit Cleanup"
    );
    assert!(!f.path().join(".git/index.lock").exists());
    Ok(())
}
#[test]
fn shelves_keep_independent_index_work_untracked_and_permissions_and_refuse_new_edits()
-> Result<(), String> {
    let f = Fixture::new()?;
    f.write("a", b"a staged\n")?;
    f.git(&["add", "a"])?;
    f.write("a", b"a working\n")?;
    f.write("new", b"new data\n")?;
    f.write("b", b"other staged\n")?;
    f.git(&["add", "b"])?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(f.path().join("new"), std::fs::Permissions::from_mode(0o640))
            .map_err(|e| e.to_string())?;
    }
    let id = f.create("Feature", &["a", "new"])?;
    let other = f.git(&["rev-parse", ":b"])?;
    shelve(f.path(), &f.read()?, &id, false, &|| true)?;
    assert_eq!(f.git(&["show", ":a"])?, "a base");
    assert_eq!(
        std::fs::read(f.path().join("a")).map_err(|e| e.to_string())?,
        b"a base\n"
    );
    assert!(!f.path().join("new").exists());
    assert_eq!(f.git(&["rev-parse", ":b"])?, other);
    f.write("a", b"newer concurrent work\n")?;
    let old_index = std::fs::read(f.path().join(".git/index")).map_err(|e| e.to_string())?;
    assert!(shelve(f.path(), &f.read()?, &id, true, &|| true).is_err());
    assert_eq!(
        std::fs::read(f.path().join(".git/index")).map_err(|e| e.to_string())?,
        old_index
    );
    assert_eq!(
        std::fs::read(f.path().join("a")).map_err(|e| e.to_string())?,
        b"newer concurrent work\n"
    );
    f.write("a", b"a base\n")?;
    shelve(f.path(), &f.read()?, &id, true, &|| true)?;
    assert_eq!(f.git(&["show", ":a"])?, "a staged");
    assert_eq!(
        std::fs::read(f.path().join("a")).map_err(|e| e.to_string())?,
        b"a working\n"
    );
    assert_eq!(
        std::fs::read(f.path().join("new")).map_err(|e| e.to_string())?,
        b"new data\n"
    );
    assert_eq!(f.git(&["rev-parse", ":b"])?, other);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(f.path().join("new"))
                .map_err(|e| e.to_string())?
                .permissions()
                .mode()
                & 0o777,
            0o640
        );
    }
    Ok(())
}
#[test]
fn stale_list_preview_and_index_locks_fail_without_ref_or_file_changes() -> Result<(), String> {
    let f = Fixture::new()?;
    f.write("a", b"change\n")?;
    let id = f.create("Feature", &["a"])?;
    let v = f.read()?;
    let head = f.git(&["rev-parse", "HEAD"])?;
    f.write("a", b"concurrent work\n")?;
    assert!(commit(f.path(), &v, &id, &|| true).is_err());
    assert_eq!(f.git(&["rev-parse", "HEAD"])?, head);
    std::fs::write(f.path().join(".git/index.lock"), b"unrelated owner")
        .map_err(|e| e.to_string())?;
    assert!(commit(f.path(), &f.read()?, &id, &|| true).is_err());
    assert_eq!(
        std::fs::read(f.path().join(".git/index.lock")).map_err(|e| e.to_string())?,
        b"unrelated owner"
    );
    assert_eq!(f.git(&["rev-parse", "HEAD"])?, head);
    Ok(())
}

#[test]
fn pure_state_snapshot_budget_stops_before_fifth_eight_mib_read_without_allocating_payloads() {
    let mut b = SnapshotBudget::from_sizes([]).unwrap();
    let mut requested = Vec::new();
    for _ in 0..2000 {
        let Ok(allowance) = b.allowance() else {
            break;
        };
        requested.push(allowance);
        b.retain(FILE_LIMIT).unwrap();
    }
    assert_eq!(requested, vec![FILE_LIMIT; 4]);
    assert!(b.allowance().is_err());
    assert!(SnapshotBudget::from_sizes([usize::MAX, 1]).is_err());
    let mut b = SnapshotBudget::from_sizes([TOTAL_LIMIT - 17]).unwrap();
    assert_eq!(b.allowance().unwrap(), 17);
    assert!(b.retain(18).is_err());
    assert_eq!(b.allowance().unwrap(), 17);
    b.retain(17).unwrap();
    assert!(b.allowance().is_err());
    assert!(SnapshotBudget::from_sizes([TOTAL_LIMIT + 1]).is_err());
}

#[test]
fn pure_state_unshelve_refuses_another_active_owner_before_restoring_contents() {
    let shelf = Shelf {
        permissions: BTreeMap::new(),
        base: BTreeMap::new(),
        index: BTreeMap::new(),
        work: BTreeMap::new(),
    };
    let a = List {
        id: "a".into(),
        name: "A".into(),
        message: "draft A".into(),
        paths: ["f".into()].into(),
        shelf: Some(shelf.clone()),
    };
    let b = List {
        id: "b".into(),
        name: "B".into(),
        message: "draft B".into(),
        paths: ["f".into()].into(),
        shelf: None,
    };
    let mut lists = Lists {
        lists: vec![a.clone(), b],
    };
    let error = require_restore_ownership(&lists, &a).unwrap_err();
    assert!(error.contains("active changelist B"));
    assert!(lists.lists[0].shelf.is_some());
    assert!(lists.lists[1].paths.contains("f"));
    assert_eq!(lists.lists[0].message, "draft A");
    lists.lists[1].paths.clear();
    assert!(require_restore_ownership(&lists, &a).is_ok());
    lists.lists[1].paths.insert("f".into());
    lists.lists[1].shelf = Some(shelf);
    assert!(require_restore_ownership(&lists, &a).is_ok());
}

#[test]
fn pure_state_selected_index_records_do_not_reconstitute_unselected_entries() {
    let selected = [
        (
            "chosen".into(),
            Some(Entry {
                mode: "100755".into(),
                oid: "a".repeat(40),
            }),
        ),
        ("deleted".into(), None),
    ]
    .into();
    let records = selected_index_records(&selected, 40);
    let rows = records
        .split(|b| *b == 0)
        .filter(|r| !r.is_empty())
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), 2);
    assert_eq!(
        rows[0],
        format!("100755 {}\tchosen", "a".repeat(40)).as_bytes()
    );
    assert_eq!(rows[1], format!("0 {}\tdeleted", "0".repeat(40)).as_bytes());
    assert!(!records.windows(6).any(|w| w == b"intent"));
    assert_eq!(selected_index_records(&BTreeMap::new(), 64), b"");
}

// Owned Git regression: compiled while runtime fixtures are globally held.
#[test]
fn changelist_commit_preserves_unselected_intent_to_add_flag() -> Result<(), String> {
    let f = Fixture::new()?;
    f.write("intent", b"not staged\n")?;
    f.git(&["add", "-N", "intent"])?;
    let before = f.git(&["ls-files", "--debug", "intent"])?;
    f.write("a", b"chosen change\n")?;
    let id = f.create("chosen", &["a"])?;
    commit(f.path(), &f.read()?, &id, &|| true)?;
    let after = f.git(&["ls-files", "--debug", "intent"])?;
    assert_eq!(after, before);
    assert_eq!(f.git(&["diff", "--cached", "--name-only"])?, "");
    assert_eq!(f.git(&["diff", "--name-only"])?, "intent");
    Ok(())
}

#[test]
fn pure_state_direct_ref_prepared_guard_covers_state_and_late_symbolic_destination() {
    let refs = ["refs/gitcraft/changelists/work/state", "refs/heads/alpha"];
    let script = direct_ref_guard(refs);
    for reference in refs {
        assert!(script.contains(&format!(
            "git symbolic-ref -q {}",
            remote::sh_quote(reference)
        )));
    }
    assert_eq!(script.matches("[ \"$code\" = 1 ] || exit 1").count(), 2);
    // Record the transaction protocol without dispatching Git. no-deref is a
    // per-command option, so every state/destination update and verify needs it.
    let old = "a".repeat(40);
    let new = "b".repeat(40);
    let tx = format!(
        "start\nupdate HEAD {old} {old}\n{}{}{}prepare\ncommit\n",
        direct_update(refs[0], &new, &old),
        direct_update(refs[1], &new, &old),
        direct_verify("refs/heads/beta", &old)
    );
    let mut direct_next = false;
    let mut commands = Vec::new();
    for line in tx.lines() {
        if line == "option no-deref" {
            direct_next = true;
            continue;
        }
        if line.starts_with("update ") || line.starts_with("verify ") {
            let reference = line.split_whitespace().nth(1).unwrap();
            commands.push((reference, direct_next));
            direct_next = false;
        }
    }
    assert_eq!(
        commands,
        vec![
            ("HEAD", false),
            (refs[0], true),
            (refs[1], true),
            ("refs/heads/beta", true)
        ]
    );
    assert!(!tx.contains("refs/heads/victim"));
}
