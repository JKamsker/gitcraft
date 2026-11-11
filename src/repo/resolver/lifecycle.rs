use super::*;

/// Removes what a run leaves, in the background (removing a worktree runs git): the agent is
/// stopped first, and only once it is gone is its folder removed (on Windows a running
/// program holds it).
pub(in crate::repo) fn clean_up(run: &mut Run) {
    if !confirm_cleanup(run) {
        return;
    }
    let workspace = run.workspace.take();
    let mcp_file = run.mcp_file.take();
    std::thread::spawn(move || {
        if let Some(file) = mcp_file {
            let _ = std::fs::remove_file(file);
        }
        // Held elsewhere (being applied, or checked), it is removed when that ends: dropping
        // the last of it removes it too.
        if let Some(ws) = workspace.and_then(|ws| Arc::try_unwrap(ws).ok()) {
            let _ = ws.remove_patiently();
        }
    });
}

/// Joining the worker and its typed exit are both required before deleting or using files.
pub(in crate::repo) fn confirm_cleanup(run: &mut Run) -> bool {
    if run.agent.as_ref().is_none_or(|a| a.stop_now()) {
        run.agent = None;
        return true;
    }
    run.phase = Phase::Failed;
    run.picked.clear();
    run.review = None;
    run.error = Some(
        "Agent cleanup could not be confirmed; workspace and recovery artifacts retained".into(),
    );
    if let Some(ws) = &run.workspace {
        let _ = ws.preserve();
        parking::preserve_uncertain(run, ws);
    }
    false
}

impl Drop for ResolverState {
    /// The tab is closed, or gitcraft is: the agent is stopped before this returns, as a
    /// thread left to do it may never run when the program ends. The rest is removed in the
    /// background (or, if gitcraft ends first, swept the next time it makes a scratch
    /// worktree).
    fn drop(&mut self) {
        if let Some(run) = &mut self.run {
            if !confirm_cleanup(run) {
                return;
            }
            // The agent's work kept with a merge still put aside stays with it: the next
            // session shows it again (see `parking`). Only what is this session's goes.
            if run.parked.is_some() && run.workspace.as_ref().is_some_and(|w| w.is_kept()) {
                run.workspace = None;
            }
            clean_up(run);
        }
    }
}
