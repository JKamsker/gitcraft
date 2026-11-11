/// The restack's undo, run as the tab runs an undo.
fn run_undo(repo: &TestRepo, steps: &[Vec<String>]) -> Result<String, String> {
    let (result, _) =
        crate::repo::operations::run_steps(repo.path(), steps, crate::repo::OpKind::plain(), None);
    result
}

/// Undo neither moves a branch checked out since under its files, nor half undoes: a reset
/// that refuses comes before any branch has moved.
#[test]
fn undo_refuses_rather_than_move_a_checked_out_branch_or_stop_halfway() -> Result<(), String> {
    use crate::git::stack::restack::{Moved, Place};
    let repo = TestRepo::new("undo checked out since")?;
    repo.stack(&["b", "c"])?;
    let main = repo.tip("main")?;
    let (b, c) = (repo.tip("b")?, repo.tip("c")?);
    let moved = |place_b: Place, place_c: Place| {
        vec![
            Moved {
                branch: "b".into(),
                before: main.clone(),
                after: b.clone(),
                base_before: None,
                base_after: main.clone(),
                place: place_b,
            },
            Moved {
                branch: "c".into(),
                before: b.clone(),
                after: c.clone(),
                base_before: None,
                base_after: b.clone(),
                place: place_c,
            },
        ]
    };
    // b checked out here since the restack, which saw it nowhere.
    repo.checkout("b")?;
    let (undo, _) = super::restack::undo_steps(&moved(Place::Nowhere, Place::Nowhere));
    assert!(run_undo(&repo, &undo).is_err());
    assert_eq!((repo.tip("b")?, repo.tip("c")?), (b.clone(), c.clone()));
    assert!(repo.git(&["status", "--porcelain"])?.trim().is_empty());
    // c checked out here, with a change its reset would lose: refused before b moves.
    repo.checkout("c")?;
    std::fs::write(repo.dir.join("c.txt"), "changed\n").map_err(|e| e.to_string())?;
    let (undo, _) = super::restack::undo_steps(&moved(Place::Nowhere, Place::Here));
    assert!(run_undo(&repo, &undo).is_err());
    assert_eq!((repo.tip("b")?, repo.tip("c")?), (b.clone(), c));
    // Without the change, it all goes back.
    repo.git(&["checkout", "--", "c.txt"])?;
    run_undo(&repo, &undo)?;
    assert_eq!((repo.tip("b")?, repo.tip("c")?), (main, b));
    Ok(())
}

/// A repository on no forge gitcraft knows has nowhere to publish to: said, nothing opened.
#[test]
fn publishing_without_a_forge_says_so() -> Result<(), String> {
    let repo = TestRepo::new("tab publish no forge")?;
    repo.stack(&["api"])?;
    let ctx = egui::Context::default();
    let mut tab = open(&repo, &ctx)?;
    settle(&mut tab, &ctx, |t| t.stacks.layer("api").is_some())?;
    tab.apply(Action::Stack(StackAct::OpenPublish("api".into())), &ctx);
    assert!(tab.stacks.publish.is_none());
    let status = tab.status.as_deref().cloned().unwrap_or_default();
    assert!(
        status.1 && status.0.contains("nowhere to publish"),
        "{status:?}"
    );
    Ok(())
}
