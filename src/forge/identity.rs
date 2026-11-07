//! Repository identity for UI lifetimes and explicit HTTP publication destinations.

use super::{Forge, Hosting};

impl Hosting {
    /// The same repository on the same web instance, irrespective of the Git transport
    /// used to detect it. A web port or reverse-proxy prefix can name another instance.
    pub(crate) fn same_repository(&self, other: &Self) -> bool {
        self.forge == other.forge
            && self.host.eq_ignore_ascii_case(&other.host)
            && self.same_repository_path(&other.path)
            && match (web(&self.web), web(&other.web)) {
                (Some(a), Some(b)) => {
                    same_authority(&a, &b)
                        && a.path().trim_end_matches('/') == b.path().trim_end_matches('/')
                }
                _ => false,
            }
    }

    pub(crate) fn same_repository_path(&self, path: &str) -> bool {
        match self.forge {
            Forge::GitLab => self.path == path,
            Forge::GitHub | Forge::Forgejo | Forge::Gitea => self.path.eq_ignore_ascii_case(path),
        }
    }

    /// An explicit HTTP Git destination must name the captured web authority and prefix.
    /// This does not infer a web port/prefix from SSH, which encodes neither of them.
    pub(crate) fn matches_http_repository(&self, url: &str) -> bool {
        let (Some(actual), Some(base)) = (web(url), web(&self.web)) else {
            return false;
        };
        if !actual.host().eq_ignore_ascii_case(&self.host) || !same_authority(&actual, &base) {
            return false;
        }
        let path = actual.path().trim_end_matches('/');
        let path = path.strip_suffix(".git").unwrap_or(path);
        // Proxy prefixes are case-sensitive even on forges whose repository names aren't.
        path.strip_prefix(base.path().trim_end_matches('/'))
            .and_then(|path| path.strip_prefix('/'))
            .is_some_and(|path| self.same_repository_path(path))
    }
}

/// Parse only: constructing a request does not send it. Unlike the web-link helper,
/// explicit HTTP URLs may name single-label hosts. URL parsing canonicalizes DNS case,
/// schemes and default ports without lowercasing the case-sensitive proxy path.
fn web(url: &str) -> Option<ureq::RequestUrl> {
    let parsed = ureq::get(url.trim()).request_url().ok()?;
    (matches!(parsed.scheme(), "http" | "https")
        && parsed.as_url().query().is_none()
        && parsed.as_url().fragment().is_none())
    .then_some(parsed)
}

fn same_authority(a: &ureq::RequestUrl, b: &ureq::RequestUrl) -> bool {
    a.scheme() == b.scheme()
        && a.host().eq_ignore_ascii_case(b.host())
        && a.as_url().port_or_known_default() == b.as_url().port_or_known_default()
}
