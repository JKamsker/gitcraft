//! Whether two runs came from the same workflow file: the file's blob at each run's revision,
//! read from Git without fetching. A revision or file the repository doesn't have is unknown,
//! not unchanged.

use std::path::Path;

use crate::ci::timing::RunInfo;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkflowFile {
    /// The same blob at both revisions.
    Same(String),
    Changed {
        before: String,
        now: String,
    },
    /// Why it isn't known.
    Unknown(String),
}

impl WorkflowFile {
    pub fn label(&self) -> String {
        match self {
            Self::Same(blob) => format!("unchanged between the two revisions (blob {})", short(blob)),
            Self::Changed { before, now } => format!(
                "changed between the two revisions (blob {} → {})",
                short(before),
                short(now)
            ),
            Self::Unknown(why) => format!("unknown ({why})"),
        }
    }
}

fn short(id: &str) -> &str {
    id.get(..7).unwrap_or(id)
}

/// The id of `path`'s blob at commit `sha`.
pub fn blob(repo: &Path, sha: &str, path: &str) -> Result<String, String> {
    if sha.is_empty() || !sha.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("no commit id was recorded".into());
    }
    if path.is_empty() || path.starts_with(['-', '/']) {
        return Err("the run didn't come from a workflow file of the repository".into());
    }
    crate::git::run_env(
        Some(repo),
        &["rev-parse", "--verify", "--quiet", &format!("{sha}:{path}")],
        &[("GIT_NO_LAZY_FETCH", "1")],
    )
    .map(|out| out.trim().to_owned())
    .ok()
    .filter(|id| !id.is_empty())
    .ok_or_else(|| format!("{path} isn't in this repository at {}", short(sha)))
}

/// The workflow file of `now` against that of `before`, each at its own revision.
pub fn compare(repo: &Path, now: &RunInfo, before: &RunInfo) -> WorkflowFile {
    match (
        blob(repo, &now.sha, &now.path),
        blob(repo, &before.sha, &before.path),
    ) {
        (Ok(a), Ok(b)) if a == b && now.path == before.path => WorkflowFile::Same(a),
        (Ok(now), Ok(before)) => WorkflowFile::Changed { before, now },
        (Err(why), _) => WorkflowFile::Unknown(format!("this run: {why}")),
        (_, Err(why)) => WorkflowFile::Unknown(format!("the earlier run: {why}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_workflow_file_is_compared_by_blob_at_each_revision() -> Result<(), String> {
        let repo = crate::git::fixture::Repo::new("ci-path-revision")?;
        let path = ".github/workflows/ci.yml";
        let first = repo.commit("ci", &[(path, Some("jobs: {a: {}}\n"))])?;
        let same = repo.commit("docs", &[("README.md", Some("x\n"))])?;
        let changed = repo.commit("ci: b", &[(path, Some("jobs: {b: {}}\n"))])?;
        let run = |sha: &str| RunInfo {
            sha: sha.to_owned(),
            path: path.to_owned(),
            ..RunInfo::default()
        };
        let unchanged = compare(repo.path(), &run(&same), &run(&first));
        assert!(matches!(unchanged, WorkflowFile::Same(_)), "{unchanged:?}");
        let moved = compare(repo.path(), &run(&changed), &run(&first));
        assert!(matches!(moved, WorkflowFile::Changed { .. }), "{moved:?}");
        assert!(moved.label().starts_with("changed"));
        let missing = compare(repo.path(), &run(&changed), &run(&"c".repeat(40)));
        assert!(
            matches!(missing, WorkflowFile::Unknown(ref why) if why.contains("earlier run")),
            "{missing:?}"
        );
        let no_file = RunInfo {
            path: String::new(),
            ..run(&first)
        };
        assert!(matches!(
            compare(repo.path(), &no_file, &run(&first)),
            WorkflowFile::Unknown(_)
        ));
        assert!(blob(repo.path(), "HEAD", path).is_err(), "only commit ids");
        Ok(())
    }
}
