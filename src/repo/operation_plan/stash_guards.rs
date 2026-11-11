//! A guard/read preceding stash push must not turn a no-op into authority over an older stash.
use super::*;

fn prefix(path: &Path, kind: &str) -> Vec<String> {
    match kind {
        "ref" => super::super::expect_ref_at("HEAD", &git::head(path)),
        "symref" => super::super::expect_symref("HEAD", "refs/heads/main"),
        "tree" => super::super::expect_tree(
            "HEAD",
            git::run(Some(path), &["rev-parse", "HEAD^{tree}"])
                .unwrap_or_default()
                .trim(),
        ),
        _ => ops(&[&["rev-parse", "--is-inside-work-tree"]]).remove(0),
    }
}

fn guarded_push(path: &Path, kind: &str) -> Vec<Vec<String>> {
    [vec![prefix(path, kind)], ops(&[&stash_push_args()])].concat()
}

#[test]
fn guarded_noop_stash_never_creates_undo_authority_over_an_older_stash() -> Result<(), String> {
    for guard in ["ref", "symref", "tree", "read"] {
        let dir = repo(&format!("guarded-noop-stash-{guard}"));
        std::fs::write(dir.join("older.txt"), b"older exact bytes\0\xff")
            .map_err(|e| e.to_string())?;
        git::run(Some(&dir), &stash_push_args())?;
        let older = git::resolve(&dir, "refs/stash")?;
        let commands = guarded_push(&dir, guard);
        let undo = Undo::Custom {
            undo: [
                vec![super::super::expect_ref("refs/stash")],
                ops(&[&["stash", "pop", "--index"]]),
            ]
            .concat(),
            redo: commands.clone(),
        };
        let (result, kind) = run_steps(&dir, &commands, OpKind::Normal(undo, String::new()), None);
        assert!(result.is_ok(), "{guard}: {result:?}");
        assert!(matches!(kind, OpKind::Normal(Undo::None, _)), "{guard}");
        assert_eq!(git::resolve(&dir, "refs/stash")?, older);
        assert!(!dir.join("older.txt").exists());
        git::run(Some(&dir), &["stash", "pop", "--index"])?;
        assert_eq!(
            std::fs::read(dir.join("older.txt")).map_err(|e| e.to_string())?,
            b"older exact bytes\0\xff"
        );
        std::fs::remove_dir_all(dir).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[test]
fn guarded_noop_redo_preserves_its_original_stash_guard_and_refuses_old_stash_undo()
-> Result<(), String> {
    for guard in ["ref", "symref", "tree", "read"] {
        let dir = repo(&format!("guarded-noop-redo-{guard}"));
        std::fs::write(dir.join("older.txt"), b"older exact bytes\0\xff")
            .map_err(|e| e.to_string())?;
        git::run(Some(&dir), &stash_push_args())?;
        let older = git::resolve(&dir, "refs/stash")?;
        let saved = stash(&dir, "intended.txt").ok_or("missing intended stash undo")?;
        let (undone, kind) = run_steps(&dir, &saved.undo, OpKind::Undo(saved.clone()), None);
        assert!(undone.is_ok(), "{guard}: {undone:?}");
        let OpKind::Undo(mut entry) = kind else {
            return Err("missing undo result".into());
        };
        std::fs::remove_file(dir.join("intended.txt")).map_err(|e| e.to_string())?;
        entry.redo = guarded_push(&dir, guard);
        let original_guard = entry.undo.clone();
        let (redone, kind) = run_steps(&dir, &entry.redo, OpKind::Redo(entry.clone()), None);
        assert!(redone.is_ok(), "{guard}: {redone:?}");
        let OpKind::Redo(entry) = kind else {
            return Err("missing redo result".into());
        };
        assert_eq!(
            entry.undo, original_guard,
            "{guard}: no-op redo must not rebind old stash"
        );
        let (refused, _) = run_steps(&dir, &entry.undo, OpKind::Undo(entry.clone()), None);
        assert!(refused.is_err(), "{guard}: old stash must not be popped");
        assert_eq!(git::resolve(&dir, "refs/stash")?, older);
        assert!(!dir.join("older.txt").exists());
        std::fs::remove_dir_all(dir).map_err(|e| e.to_string())?;
    }
    Ok(())
}
