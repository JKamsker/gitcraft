//! Repository identity for UI lifetimes and explicit HTTP publication destinations.

use super::{Forge, Hosting};

impl Hosting {
    /// The same repository on the same web instance, irrespective of the Git transport
    /// used to detect it. A web port or reverse-proxy prefix can name another instance.
    pub(crate) fn same_repository(&self, other: &Self) -> bool {
        self.forge == other.forge
            && self.host.eq_ignore_ascii_case(&other.host)
            && self.same_repository_path(&other.path)
            && same_web_instance(&self.web, &other.web)
    }

    pub(crate) fn same_repository_path(&self, path: &str) -> bool {
        match self.forge {
            Forge::GitLab | Forge::Azure | Forge::Bitbucket => self.path == path,
            Forge::GitHub | Forge::Forgejo | Forge::Gitea => self.path.eq_ignore_ascii_case(path),
        }
    }

    /// A returned or saved PR URL may never redirect a follow-up action to another instance.
    pub(crate) fn matches_pull_request(&self, url: &str) -> bool {
        crate::pull_request::repository_url(url)
            .is_some_and(|repo| self.matches_http_repository(repo))
    }

    /// HTTP explicitly names a web instance. SSH names only the host and repository path;
    /// its port is independent of the captured web server and reverse-proxy prefix.
    pub(crate) fn matches_git_remote(&self, url: &str) -> bool {
        let url = url.trim();
        if let Some((scheme, _)) = url.split_once("://") {
            if scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https") {
                return self.matches_http_repository(url);
            }
            if !scheme.eq_ignore_ascii_case("ssh") {
                return false;
            }
        }
        super::location(url).is_some_and(|(host, path)| {
            host.eq_ignore_ascii_case(&self.host) && self.same_repository_path(&path)
        })
    }

    /// Persisted review state uses the same identity as the in-memory repository lifetime.
    /// Legacy host/path keys cannot safely be attributed to a particular web instance.
    pub(crate) fn review_repository_key(&self) -> Option<String> {
        use sha2::{Digest, Sha256};
        let base = web(&self.web)?;
        let path = if matches!(self.forge, Forge::GitLab | Forge::Azure | Forge::Bitbucket) {
            self.path.clone()
        } else {
            self.path.to_ascii_lowercase()
        };
        let identity = serde_json::to_vec(&(
            self.forge.key(),
            base.scheme(),
            base.host(),
            base.as_url().port_or_known_default(),
            base.path().trim_end_matches('/'),
            path,
        ))
        .ok()?;
        // Git sanitizes ref components; hash the tuple so distinct valid URL paths cannot
        // collapse there (for example `/reviews:prod` versus `/reviews_prod`).
        let digest: String = Sha256::digest(identity)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        Some(format!("v2/{digest}"))
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

pub(super) fn same_web_instance(a: &str, b: &str) -> bool {
    match (web(a), web(b)) {
        (Some(a), Some(b)) => {
            same_authority(&a, &b)
                && a.path().trim_end_matches('/') == b.path().trim_end_matches('/')
        }
        _ => false,
    }
}

fn same_authority(a: &ureq::RequestUrl, b: &ureq::RequestUrl) -> bool {
    a.scheme() == b.scheme()
        && a.host().eq_ignore_ascii_case(b.host())
        && a.as_url().port_or_known_default() == b.as_url().port_or_known_default()
}

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[test]
    fn added_forges_keep_case_sensitive_paths_and_exact_web_instances() {
        for forge in [Forge::Azure, Forge::Bitbucket] {
            let h = Hosting {
                forge,
                host: "forge.example".into(),
                path: "Team/App".into(),
                web: "https://forge.example/proxy".into(),
            };
            assert!(h.same_repository(&h.clone()));
            assert!(h.matches_git_remote("ssh://git@forge.example/Team/App"));
            assert!(h.matches_http_repository("https://FORGE.example:443/proxy/Team/App.git"));
            for url in [
                "https://forge.example/proxy/team/app",
                "https://forge.example:8443/proxy/Team/App",
                "http://forge.example/proxy/Team/App",
                "https://forge.example/other/Team/App",
            ] {
                assert!(!h.matches_http_repository(url), "{forge:?} accepted {url}");
            }
            let other = Hosting {
                path: "team/app".into(),
                ..h.clone()
            };
            assert!(!h.same_repository(&other));
            assert_ne!(h.review_repository_key(), other.review_repository_key());
        }
    }
}
