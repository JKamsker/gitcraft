// Hover regression coverage.

/// A hover that ends mid-way stops the work.
#[test]
fn a_cancelled_ghost_run_stops() -> Result<(), String> {
    let repo = diverged("ghost-cancel")?;
    let facts = facts(&repo)?;
    let op = GhostOp::Rebase {
        onto: "refs/heads/main".to_owned(),
        label: "main".to_owned(),
    };
    let out = plan(&repo.dir, &op, &facts, GitVersion(2, 41, 0), &|| true);
    assert_eq!(out.err().as_deref(), Some(crate::git::dry_run::CANCELLED));
    Ok(())
}

/// Waits for the tab's background work to come in, until `done`.
fn wait(
    tab: &mut crate::repo::RepoTab,
    ctx: &egui::Context,
    done: impl Fn(&crate::repo::RepoTab) -> bool,
) {
    for _ in 0..400 {
        tab.poll(ctx);
        if done(tab) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
}

/// Offers `op` as a menu entry under the pointer does, for the frame `ghost_frame` runs in.
fn offer_now(ctx: &egui::Context, op: &GhostOp) {
    let pass = ctx.cumulative_pass_nr();
    ctx.data_mut(|d| d.insert_temp(super::offer_id(), (pass, op.clone())));
}

/// The pointer leaves the entry (a later frame offers nothing).
fn leave(ctx: &egui::Context) {
    ctx.data_mut(|d| d.remove::<(u64, GhostOp)>(super::offer_id()));
}

/// Hovering an entry starts nothing until the delay has passed; then the ghost graph comes in
/// the background, with the rebase's copies on top; once the pointer leaves, it goes and the
/// work is told to stop. The repository is as it was.
#[test]
fn a_hover_shows_the_ghost_graph_and_leaving_ends_it() -> Result<(), String> {
    let repo = diverged("ghost-hover")?;
    let ctx = egui::Context::default();
    let mut tab = crate::repo::RepoTab::new(repo.dir.clone(), &ctx);
    wait(&mut tab, &ctx, |t| t.history_gen > 0 && !t.loading);
    let rows = tab.commits.len();
    let before = repo.snapshot()?;
    let op = GhostOp::Rebase {
        onto: "refs/heads/main".to_owned(),
        label: "main".to_owned(),
    };
    offer_now(&ctx, &op);
    tab.ghost_frame(&ctx);
    assert!(tab.ghost.job.is_none(), "started before the delay");
    // The pointer has rested on it long enough.
    if let Some((_, at)) = &mut tab.ghost.hover {
        *at -= super::HOVER_DELAY;
    }
    offer_now(&ctx, &op);
    tab.ghost_frame(&ctx);
    let cancel = tab
        .ghost
        .job
        .as_ref()
        .map(|j| std::sync::Arc::clone(&j.cancel));
    assert!(cancel.is_some(), "no ghost run started");
    wait(&mut tab, &ctx, |t| t.ghost.shown.is_some());
    assert!(matches!(
        tab.ghost_strip_state(),
        Some(super::Strip::Ready(..))
    ));
    let (mut commits, mut graph, mut index) = (
        tab.commits.clone(),
        std::mem::take(&mut tab.graph),
        tab.index.clone(),
    );
    assert!(tab.ghost.swap_in(&mut commits, &mut graph, &mut index));
    assert_eq!(commits.len(), rows + 2);
    assert_eq!(tab.ghost.kind(0), super::Kind::New);
    tab.ghost.swap_out(&mut commits, &mut graph, &mut index);
    assert_eq!(commits.len(), rows);
    tab.graph = graph;
    // The same entry again: from the cache, nothing worked out again.
    let key = tab.ghost_key(op.clone());
    assert!(tab.ghost.cached(&key).is_some());
    // The pointer leaves.
    leave(&ctx);
    tab.ghost_frame(&ctx);
    assert!(tab.ghost.shown.is_none() && tab.ghost.hover.is_none());
    assert_eq!(repo.snapshot()?, before);
    Ok(())
}

/// A hover that ends while the work runs tells it to stop, and what it finds isn't shown.
#[test]
fn a_hover_that_ends_early_stops_its_work() -> Result<(), String> {
    let repo = diverged("ghost-hover-cancel")?;
    let ctx = egui::Context::default();
    let mut tab = crate::repo::RepoTab::new(repo.dir.clone(), &ctx);
    wait(&mut tab, &ctx, |t| t.history_gen > 0 && !t.loading);
    let op = GhostOp::ResetHard {
        to: repo.rev("HEAD~1")?,
    };
    offer_now(&ctx, &op);
    ctx.input_mut(|i| i.modifiers.alt = true);
    tab.ghost_frame(&ctx);
    if let Some((_, at)) = &mut tab.ghost.hover {
        *at -= super::ALT_DELAY;
    }
    offer_now(&ctx, &op);
    tab.ghost_frame(&ctx);
    let cancel = tab
        .ghost
        .job
        .as_ref()
        .map(|j| std::sync::Arc::clone(&j.cancel));
    leave(&ctx);
    tab.ghost_frame(&ctx);
    assert!(cancel.is_some_and(|c| c.load(std::sync::atomic::Ordering::Relaxed)));
    std::thread::sleep(std::time::Duration::from_millis(300));
    tab.poll(&ctx);
    assert!(tab.ghost.shown.is_none());
    Ok(())
}
