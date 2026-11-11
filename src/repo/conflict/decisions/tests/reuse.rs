#[test]
fn the_same_conflict_comes_back_resolved_with_its_reason() -> Result<(), String> {
    let dir = repo("again")?;
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    let mut ed = editor(&dir, "f.rs", FILE)?;
    ed.choose(0, Choice::OursThenTheirs);
    ed.memo.drafts = vec![Draft {
        open: false,
        note: "  keep the new validation, keep the old wire format ".into(),
        test: "true\n".into(),
    }];
    tab.conflict = Some(ed);
    tab.prepare_decisions();
    tab.record_decisions("f.rs", &ctx);
    wait(&mut tab, &ctx, |tab| tab.status.is_some());
    let kept = tab.status.as_deref().cloned();

    // The same conflict, elsewhere in another file.
    let elsewhere = format!("0\n{FILE}");
    tab.conflict = Some(editor(&dir, "g.rs", &elsewhere)?);
    tab.status = None;
    tab.decisions_act(Act::Load, &ctx);
    wait(&mut tab, &ctx, |tab| {
        tab.conflict
            .as_ref()
            .is_some_and(|ed| ed.memo.offers.is_some())
    });
    let _ = std::fs::remove_dir_all(&dir);

    assert_eq!(kept, Some(("Kept the decision".to_owned(), false)));
    let ed = tab.conflict.as_ref().ok_or("no editor")?;
    assert_eq!(ed.choices, vec![Some(Choice::Recorded)]);
    assert_eq!(
        ed.result,
        "0\n1\n2\n3\nnew validation\nold wire format\n4\n5\n6\n"
    );
    let offer = ed.offer(0).ok_or("nothing offered")?;
    assert!(!offer.stale);
    assert_eq!(
        offer.decision.note,
        "keep the new validation, keep the old wire format"
    );
    assert_eq!(offer.decision.test, "true");
    assert_eq!(offer.decision.by, "t");
    // Its note isn't taken as a new one of the user's: nothing to record again.
    assert!(ed.memo.drafts.iter().all(Draft::is_empty));
    assert_eq!(ed.to_record("me", 1), Ok(Vec::new()));
    assert!(
        tab.status
            .as_deref()
            .is_some_and(|(s, _)| s.contains("Applied a decision"))
    );
    Ok(())
}

/// The code around the conflict changed since: offered as stale, not applied, until asked.
#[test]
fn a_decision_whose_surroundings_changed_is_only_offered() -> Result<(), String> {
    let dir = repo("stale")?;
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    let mut ed = editor(&dir, "f.rs", FILE)?;
    ed.choose(0, Choice::Theirs);
    ed.memo.drafts = vec![Draft {
        note: "the wire format wins".into(),
        ..Draft::default()
    }];
    tab.conflict = Some(ed);
    tab.prepare_decisions();
    tab.record_decisions("f.rs", &ctx);
    wait(&mut tab, &ctx, |tab| tab.status.is_some());

    let moved = FILE.replace("3\n", "three\n");
    tab.conflict = Some(editor(&dir, "f.rs", &moved)?);
    tab.decisions_act(Act::Load, &ctx);
    wait(&mut tab, &ctx, |tab| {
        tab.conflict
            .as_ref()
            .is_some_and(|ed| ed.memo.offers.is_some())
    });
    let _ = std::fs::remove_dir_all(&dir);
    let ed = tab.conflict.as_mut().ok_or("no editor")?;
    let offer = ed.offer(0).ok_or("nothing offered")?.clone();
    assert!(offer.stale);
    assert_eq!(offer.now.0, "1\n2\nthree\n");
    assert_eq!(offer.decision.before, "1\n2\n3\n");
    assert_eq!(ed.choices, vec![None]);
    ed.use_offer(0);
    assert_eq!(ed.choices, vec![Some(Choice::Recorded)]);
    assert!(ed.result.contains("1\n2\nthree\nold wire format\n4\n"));
    Ok(())
}
