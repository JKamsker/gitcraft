use super::super::Hosting;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Query {
    Mine,
    Starred,
    OwnerRepositories { login: String, organization: bool },
    SearchOwners(String),
    SearchRepositories(String),
}
impl Query {
    pub fn public(&self) -> bool {
        !matches!(self, Self::Mine | Self::Starred)
    }
}
#[derive(Clone, Debug)]
pub(crate) struct Owner {
    pub id: u64,
    pub login: String,
    pub organization: bool,
}
#[derive(Clone, Debug)]
pub(crate) struct Repository {
    pub id: u64,
    pub hosting: Hosting,
    pub owner: String,
    pub name: String,
    pub description: String,
    pub language: Option<String>,
    pub stars: Option<u64>,
    pub visibility: Visibility,
    pub web_url: String,
    pub https_url: String,
    pub ssh_url: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Visibility {
    Public,
    Private,
    Internal,
}
#[derive(Default, Debug)]
pub(crate) struct RateLimit {
    pub remaining: Option<u64>,
    pub reset: Option<u64>,
    pub retry_after: Option<u64>,
}
#[derive(Debug)]
pub(crate) struct Page {
    pub repositories: Vec<Repository>,
    pub owners: Vec<Owner>,
    pub next: Option<String>,
    pub total: Option<u64>,
    pub incomplete: bool,
    pub rate: RateLimit,
}
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum DiscoveryError {
    AuthRequired,
    AuthRefused,
    Forbidden,
    NotFound,
    RateLimited { retry_at: Option<u64> },
    Transport,
    Malformed,
    Cancelled,
    QueueUnavailable,
}
