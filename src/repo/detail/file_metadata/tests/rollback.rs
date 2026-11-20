//! The case rename through the operation executor: it goes through, undoes, and when its
//! second move fails, the file is moved back and the operation fails.

use super::*;
use crate::git::file_metadata as meta;

fn index_paths(repo: &Repo) -> Result<Vec<String>, String> {
    Ok(repo
        .git(&["ls-files", "-z"])?
        .split('\0')
        .filter(|p| !p.is_empty())
        .map(str::to_owned)
        .collect())
}

#[test]
fn guarded_case_rename_runs_and_undoes_through_the_executor() -> Result<(), String> {
    let repo = Repo::new("file-metadata-rollback-ok")?;
    repo.commit("base", &[("Name.rs", Some("one\n"))])?;
    std::fs::write(repo.dir.join("Name.rs"), "one\ntwo\n").map_err(|e| e.to_string())?;
    let snapshot = meta::snapshot(repo.path(), "Name.rs", true)?;
    let plan = flow::rolled_back(meta::case_rename_plan(&snapshot, "name.rs")?);
    run_steps(repo.path(), &plan.ops, OpKind::plain(), None).0?;
    assert_eq!(index_paths(&repo)?, ["name.rs"]);
    run_steps(repo.path(), &plan.undo, OpKind::plain(), None).0?;
    assert_eq!(index_paths(&repo)?, ["Name.rs"]);
    let kept = std::fs::read(repo.dir.join("Name.rs")).map_err(|e| e.to_string())?;
    assert_eq!(kept, b"one\ntwo\n");
    Ok(())
}

/// The second move is made to fail (its folder doesn't exist): the file is moved back from
/// the temporary name, with its unstaged edit, and the operation reports a failure.
#[test]
fn a_failed_second_move_puts_the_file_back_and_fails_the_operation() -> Result<(), String> {
    let repo = Repo::new("file-metadata-rollback-failed")?;
    repo.commit("base", &[("Name.rs", Some("one\n"))])?;
    let edited = b"one\r\nedited \xc3\xbc\n";
    std::fs::write(repo.dir.join("Name.rs"), edited).map_err(|e| e.to_string())?;
    let via = ".Name.rs.gitcraft-case-rename".to_owned();
    let failing = meta::Plan {
        label: "Rename Name.rs to missing/name.rs".into(),
        ops: Vec::new(),
        undo: Vec::new(),
        from: "Name.rs".into(),
        to: "missing/name.rs".into(),
        executable: false,
        via: Some(via.clone()),
    };
    let plan = flow::rolled_back(failing);
    let (result, _) = run_steps(repo.path(), &plan.ops, OpKind::plain(), None);
    // The last check failed (it names the new path): the first move and the move back ran.
    assert!(
        result
            .as_ref()
            .is_err_and(|e| e.contains("missing/name.rs") && !e.contains("bad source")),
        "{result:?}"
    );
    assert_eq!(index_paths(&repo)?, ["Name.rs"]);
    let back = std::fs::read(repo.dir.join("Name.rs")).map_err(|e| e.to_string())?;
    assert_eq!(back, edited);
    assert!(!repo.dir.join(&via).exists());
    Ok(())
}
