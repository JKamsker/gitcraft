//! Repository discovery is independent of any local checkout or repository provider.
mod model;
pub(crate) use model::*;
mod public_http;
pub(super) use public_http::*;

pub(crate) trait RepositoryDiscovery {
    fn repositories(&self, query: &Query, next: Option<&str>) -> Result<Page, DiscoveryError>;
}

/// Nonsecret selection captured at dialog creation. Tokens exist only in workers.
#[derive(Clone)]
pub(crate) struct DiscoverySession {
    pub account: crate::connections::Account,
    pub generation: u64,
}
impl DiscoverySession {
    pub fn current() -> Self {
        let generation = crate::connections::generation();
        let account = crate::connections::current().choices.github;
        Self {
            account,
            generation: if generation == crate::connections::generation() {
                generation
            } else {
                u64::MAX
            },
        }
    }
    pub fn valid(&self) -> bool {
        self.generation != u64::MAX && self.generation == crate::connections::generation()
    }
}
impl RepositoryDiscovery for DiscoverySession {
    fn repositories(&self, query: &Query, next: Option<&str>) -> Result<Page, DiscoveryError> {
        if !self.valid() || crate::tasks::cancellation().is_cancelled() {
            return Err(DiscoveryError::Cancelled);
        }
        super::github::discovery::repositories(self, query, next)
    }
}
