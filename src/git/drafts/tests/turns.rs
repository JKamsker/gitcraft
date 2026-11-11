//! Turns.

use super::*;

#[test]
pub(super) fn one_turn_reversal_retains_later_hunks_and_index_and_rejects_stale_overlap()
-> Result<(), String> {
    let f = Fixture::new()?;
    let base = (1..=30).map(|i| format!("line {i}\n")).collect::<String>();
    f.write("file", base.as_bytes())?;
    f.write("other", b"original\n")?;
    f.commit()?;
    let parent = git::run(Some(&f.0), &["rev-parse", "HEAD"])?
        .trim()
        .to_owned();
    let changed = base.replace("line 3\n", "turn first\n");
    f.write("file", changed.as_bytes())?;
    f.commit()?;
    let turn = git::run(Some(&f.0), &["rev-parse", "HEAD"])?
        .trim()
        .to_owned();
    let later = changed.replace("line 25\n", "later retained\n");
    f.write("file", later.as_bytes())?;
    f.write("other", b"unrelated staged\n")?;
    git::run(Some(&f.0), &["add", "other"])?;
    f.write("untracked", b"preserve\n")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(f.0.join("file"), std::fs::Permissions::from_mode(0o640))
            .map_err(|e| e.to_string())?;
    }
    let index = std::fs::read(f.0.join(".git/index")).map_err(|e| e.to_string())?;
    let p = turn::inverse_preview(&f.0, &parent, &turn, &|| true)?;
    let result = turn::inverse_apply(&f.0, &p, &|| true);
    if cfg!(windows) {
        let error = result.err().ok_or("Windows turn reversal must refuse")?;
        assert!(
            error.contains("Native working-folder publication is unavailable"),
            "{error}"
        );
        assert_eq!(
            std::fs::read(f.0.join("file")).map_err(|e| e.to_string())?,
            later.as_bytes()
        );
        assert_eq!(
            std::fs::read(f.0.join(".git/index")).map_err(|e| e.to_string())?,
            index
        );
        assert_eq!(
            std::fs::read(f.0.join("untracked")).map_err(|e| e.to_string())?,
            b"preserve\n"
        );
        return Ok(());
    }
    let backup = result?;
    assert!(list(&f.0, &|| true)?.iter().any(|d| d.id == backup.draft));
    assert_eq!(
        std::fs::read(f.0.join("file")).map_err(|e| e.to_string())?,
        base.replace("line 25\n", "later retained\n").as_bytes()
    );
    assert_eq!(
        std::fs::read(f.0.join(".git/index")).map_err(|e| e.to_string())?,
        index
    );
    assert_eq!(
        std::fs::read(f.0.join("untracked")).map_err(|e| e.to_string())?,
        b"preserve\n"
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
            0o640
        );
    }
    f.write("file", later.as_bytes())?;
    let stale = turn::inverse_preview(&f.0, &parent, &turn, &|| true)?;
    f.write("file", b"new concurrent edit\n")?;
    assert!(turn::inverse_apply(&f.0, &stale, &|| true).is_err());
    assert_eq!(
        std::fs::read(f.0.join("file")).map_err(|e| e.to_string())?,
        b"new concurrent edit\n"
    );
    assert!(turn::inverse_preview(&f.0, &parent, &turn, &|| true).is_err());
    Ok(())
}

#[test]
pub(super) fn review_marks_persist_unchanged_hunks_but_changed_hunks_reappear() -> Result<(), String>
{
    let f = Fixture::new()?;
    let base = (1..=40).map(|i| format!("line {i}\n")).collect::<String>();
    f.write("file", base.as_bytes())?;
    f.commit()?;
    let parent = git::run(Some(&f.0), &["rev-parse", "HEAD"])?
        .trim()
        .to_owned();
    let first = base.replace("line 3\n", "review me\n");
    f.write("file", first.as_bytes())?;
    f.commit()?;
    let one = git::run(Some(&f.0), &["rev-parse", "HEAD"])?
        .trim()
        .to_owned();
    let r = turn::review(&f.0, &parent, &one, "session", &|| true)?;
    assert_eq!(r.hunks.len(), 1);
    turn::mark_reviewed(&f.0, &r, &[0], &|| true)?;
    assert!(turn::review(&f.0, &parent, &one, "session", &|| true)?.hunks[0].reviewed);
    assert!(turn::mark_reviewed(&f.0, &r, &[0], &|| true).is_err());
    f.write(
        "file",
        first.replace("line 30\n", "new change\n").as_bytes(),
    )?;
    f.commit()?;
    let two = git::run(Some(&f.0), &["rev-parse", "HEAD"])?
        .trim()
        .to_owned();
    let r = turn::review(&f.0, &parent, &two, "session", &|| true)?;
    assert_eq!(r.hunks.len(), 2);
    assert!(r.hunks[0].reviewed);
    assert!(!r.hunks[1].reviewed);
    assert!(
        turn::review(&f.0, &parent, &two, "another session", &|| true)?
            .hunks
            .iter()
            .all(|h| !h.reviewed)
    );
    Ok(())
}

#[test]
pub(super) fn file_exchange_rolls_back_owned_changes_when_guard_fails_mid_batch()
-> Result<(), String> {
    use std::cell::Cell;
    let f = Fixture::new()?;
    f.write("a", b"before a")?;
    f.write("b", b"before b")?;
    f.commit()?;
    let before = [
        ("a".into(), Some(("100644".into(), b"before a".to_vec()))),
        ("b".into(), Some(("100644".into(), b"before b".to_vec()))),
    ]
    .into_iter()
    .collect();
    let after = [
        ("a".into(), Some(("100644".into(), b"after a".to_vec()))),
        ("b".into(), Some(("100644".into(), b"after b".to_vec()))),
    ]
    .into_iter()
    .collect();
    let n = Cell::new(0);
    let check = || {
        n.set(n.get() + 1);
        if n.get() == 3 {
            Err("Injected index race".into())
        } else {
            Ok(())
        }
    };
    let policies = native_files::capture_policies(&f.0, &before, &|| true)?;
    assert!(
        working_transaction::apply(&f.0, &before, &after, &policies, &check, &|| true).is_err()
    );
    assert_eq!(
        std::fs::read(f.0.join("a")).map_err(|e| e.to_string())?,
        b"before a"
    );
    assert_eq!(
        std::fs::read(f.0.join("b")).map_err(|e| e.to_string())?,
        b"before b"
    );
    Ok(())
}
