//! Approval previews use the same dry-run planner as menu ghost runs, in the planning
//! worker. They cannot execute the requested write or approve it.
use crate::repo::ghost::{self, GhostOp};
use crate::{
    git,
    mcp::wire::{ResetMode, WriteOp},
};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Preview {
    pub moves: Vec<(String, String, String)>,
    pub leaving: Vec<String>,
    pub notes: Vec<String>,
}

pub(super) fn read(
    repo: &std::path::Path,
    op: &WriteOp,
    cancelled: &dyn Fn() -> bool,
) -> Option<Preview> {
    let result = (|| {
        let refs = git::refs(repo)?;
        let op = match op {
            WriteOp::Reset {
                commit,
                mode: ResetMode::Hard,
            } => GhostOp::ResetHard { to: commit.clone() },
            WriteOp::DeleteBranch { name, .. } => GhostOp::DeleteBranch {
                branch: name.clone(),
                remote: false,
            },
            WriteOp::DropCommit { commit } => GhostOp::DropCommit {
                hash: commit.clone(),
            },
            WriteOp::Push { force: true } => GhostOp::ForcePush {
                branch: refs.current.clone().ok_or("Detached HEAD")?,
            },
            _ => return Ok(None),
        };
        let facts = ghost::op::Facts {
            refs,
            remotes: git::remotes(repo)?.into_iter().map(|r| r.name).collect(),
            dirty: ghost::op::tracked_changes(&git::status(repo)?),
        };
        ghost::approval_preview(repo, &op, &facts, cancelled).map(Some)
    })();
    match result {
        Ok(preview) => preview,
        Err(e) => Some(Preview {
            notes: vec![format!("Ghost preview unavailable: {e}")],
            ..Default::default()
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_pending_delete_previews_its_exact_ref_without_mutating() -> Result<(), String> {
        let r = git::fixture::Repo::new("agent-ghost-preview")?;
        let oid = r.commit("base", &[])?;
        r.git(&["branch", "candidate"])?;
        let p = read(
            r.path(),
            &WriteOp::DeleteBranch {
                name: "candidate".into(),
                force: true,
            },
            &|| false,
        )
        .ok_or("No preview")?;
        assert!(
            p.moves
                .iter()
                .any(|(name, from, to)| name == "refs/heads/candidate"
                    && from == &oid
                    && to == "absent")
        );
        assert_eq!(r.rev("candidate")?, oid);
        let reset = read(
            r.path(),
            &WriteOp::Reset {
                commit: oid.clone(),
                mode: ResetMode::Hard,
            },
            &|| false,
        )
        .ok_or("No reset preview")?;
        assert!(
            !reset.notes.iter().any(|s| s.contains("unavailable")),
            "{reset:?}"
        );
        assert_eq!(r.rev("HEAD")?, oid);
        Ok(())
    }
}
