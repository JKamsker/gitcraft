use super::*;

/// What the run needs to start the agent, made in the background.
pub(super) struct Prepared {
    pub(super) workspace: Workspace,
    pub(super) mcp_file: PathBuf,
    pub(super) command: crate::ai::run::CommandLine,
    pub(super) prompt: String,
    /// The merge, put aside (when asked to and it could be).
    pub(super) parked: Option<Parked>,
    /// Why it wasn't put aside, when it was asked to be.
    pub(super) not_parked: Option<String>,
    /// The conflicted files behind the agent fence, left to the user.
    pub(super) fenced: Vec<String>,
}

/// `files` split into those the agent may have and those behind the agent fence of `repo`
/// (see [`crate::fence`]).
pub(super) fn split_by_fence(
    repo: &Path,
    files: &[String],
) -> Result<(Vec<String>, Vec<String>), String> {
    let patterns = crate::fence::parse_list(&crate::fence::read(repo)?);
    Ok(files
        .iter()
        .cloned()
        .partition(|f| !patterns.iter().any(|p| p.matches(f))))
}

/// Why fenced files can't go to the agent.
pub(super) fn behind_fence(fenced: &[String]) -> String {
    let (are, them) = if fenced.len() == 1 {
        ("is", "it")
    } else {
        ("are", "them")
    };
    format!(
        "{} {are} behind the agent fence (see Agent fence…): resolve {them} yourself",
        fenced.join(", ")
    )
}

/// Makes the agent's worktree and command for resolving `files`; then, when `put_aside`, puts
/// the merge aside (see `park`), the worktree free meanwhile. A merge that can't be (a rebase)
/// is resolved in place, and says why. Files behind the agent fence never reach the agent:
/// all of them are left to the user (the merge still put aside), and one asked for is refused.
pub(super) fn prepare(
    repo: &Path,
    files: &[String],
    engine: Engine,
    id: &str,
    put_aside: bool,
) -> Result<Prepared, String> {
    let (open, fenced) = split_by_fence(repo, files)?;
    if !fenced.is_empty() && (!put_aside || open.is_empty()) {
        return Err(behind_fence(&fenced));
    }
    let exe = std::env::current_exe().map_err(|e| format!("gitcraft can't find itself: {e}"))?;
    let workspace = Workspace::prepare(repo, &open)?;
    let mcp_file = std::env::temp_dir().join(format!("gitcraft-resolve-{id}.json"));
    // The worktree goes with `workspace` on failure (dropping it removes it); the file here.
    let command = agent::command(engine, &exe, workspace.dir(), id, &mcp_file)?;
    let config = agent::claude_mcp_config(&exe, workspace.dir(), id);
    if let Err(e) = std::fs::write(&mcp_file, config.to_string()) {
        let _ = std::fs::remove_file(&mcp_file);
        return Err(format!("couldn't write the agent's MCP settings: {e}"));
    }
    let prompt = agent::prompt(&workspace, engine);
    // The tests' stand-in for the agent, when one is set.
    #[cfg(test)]
    let command = flow_tests::command_for(repo).unwrap_or(command);
    // Last: once the agent's copy of it is made, the merge is put aside.
    let (parked, not_parked) = match put_aside.then(|| park::park(repo, id, files)) {
        Some(Ok(parked)) => (Some(parked), None),
        Some(Err(why)) => (None, Some(why)),
        None => (None, None),
    };
    Ok(Prepared {
        workspace,
        mcp_file,
        command,
        prompt,
        parked,
        not_parked,
        fenced,
    })
}
