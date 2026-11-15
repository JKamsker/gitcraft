/// Serializable for the list kept until the forge is asked again (`sidebar::pull_requests`).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PullRequest {
    pub number: u64,
    pub title: String,
    /// The branch it proposes (head / source branch).
    pub branch: String,
    /// The branch it goes into (base / target branch).
    pub base: String,
    /// The base tip observed with `sha`, when the list supplies exact diff endpoints.
    /// Older persisted lists and providers without it retain the details lookup.
    #[serde(default)]
    pub base_oid: String,
    /// It proposes a branch of another repository (a fork), which may share a name with one
    /// here.
    pub from_fork: bool,
    /// Exact head repository identity; absent for old caches or deleted forks.
    #[serde(default)]
    pub head_repo: Option<String>,
    /// Its latest commit.
    pub sha: String,
    pub author: String,
    pub url: String,
    pub draft: bool,
}
