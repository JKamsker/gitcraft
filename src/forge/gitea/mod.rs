//! Forgejo and Gitea. Forgejo started as Gitea and keeps its API, so one module serves both: the
//! list of pull requests, opening one, the page of one in full ([`details`], [`post`]) and the
//! issues assigned to the user ([`issues`]), all through the REST API at `https://host/api/v1`.
//!
//! The login is one the user already has for the host, looked for in this order:
//! 1. `GITEA_SERVER_URL` and `GITEA_SERVER_TOKEN` in the environment (as `tea` reads them), when
//!    the URL is the host's;
//! 2. the logins of `tea`, Gitea's command-line tool (`tea login add`), which works with Forgejo
//!    too;
//! 3. what git's credential helper keeps for `https://host` (a password or an access token).
//!
//! Without one, public repositories can still be read. The start page's inbox, which asks
//! unasked, takes only the first two or a login found before (see [`known_login`]).

mod details;
mod inbox;
mod issues;
#[cfg(test)]
mod live;
#[cfg(test)]
mod live_inbox;
#[cfg(test)]
mod live_issues;
mod post;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::http::{Api, Login};
use super::inbox::Answer;
use super::{
    Forge, ForkInfo, Hosting, Issue, MergedPullRequest, NewPullRequest, Pages, PrState, Provider,
    PullRequest,
};
use crate::avatars::Hint;
use crate::pr_post::Target;
use crate::pull_request::{PrDetails, at};

pub(super) struct GiteaFamily {
    forge: Forge,
    name: &'static str,
    /// Hosts known to run it.
    hosts: &'static [&'static str],
    /// A word in a host's name that says it runs it (`forgejo.example.org`).
    word: &'static str,
}

pub(super) static FORGEJO: GiteaFamily = GiteaFamily {
    forge: Forge::Forgejo,
    name: "Forgejo",
    hosts: &["codeberg.org", "next.forgejo.org", "code.forgejo.org"],
    word: "forgejo",
};

pub(super) static GITEA: GiteaFamily = GiteaFamily {
    forge: Forge::Gitea,
    name: "Gitea",
    hosts: &["gitea.com"],
    word: "gitea",
};

impl GiteaFamily {
    fn api(&self, hosting: &Hosting) -> Api {
        Api::new(
            self.name,
            &hosting.web,
            &hosting.host,
            find_login(&hosting.web),
        )
        .on_refused_key(forget_login, &login_key(&hosting.web))
    }
}

impl Provider for GiteaFamily {
    fn name(&self) -> &'static str {
        self.name
    }

    fn claims(&self, host: &str) -> bool {
        self.hosts.contains(&host) || host.contains(self.word)
    }

    fn list(&self, _: &Path, hosting: &Hosting) -> Result<Vec<PullRequest>, String> {
        list(&self.api(hosting), hosting)
    }

    fn merged(&self, _: &Path, hosting: &Hosting) -> Result<Vec<MergedPullRequest>, String> {
        merged(&self.api(hosting), hosting)
    }

    fn create(&self, _: &Path, hosting: &Hosting, new: &NewPullRequest) -> Result<String, String> {
        create(&self.api(hosting), hosting, new)
    }

    fn assigned_issues(&self, _: &Path, hosting: &Hosting) -> Result<Vec<Issue>, String> {
        issues::assigned(&self.api(hosting), hosting)
    }

    fn issue(&self, _: &Path, hosting: &Hosting, number: u64) -> Result<Issue, String> {
        issues::one(&self.api(hosting), hosting, number)
    }

    fn retarget(&self, _: &Path, hosting: &Hosting, number: u64, base: &str) -> Result<(), String> {
        retarget(&self.api(hosting), hosting, number, base)
    }

    fn find(
        &self,
        _: &Path,
        hosting: &Hosting,
        branch: &str,
    ) -> Result<Option<super::Found>, String> {
        find(&self.api(hosting), hosting, branch)
    }

    /// The accounts the latest commits belong to, as the API ties them to their emails.
    fn avatars(&self, _: &Path, hosting: &Hosting, _: &[String]) -> Vec<(String, Hint)> {
        let endpoint = format!(
            "repos/{}/commits?limit=50&stat=false&verification=false&files=false",
            hosting.path
        );
        let Ok(answer) = self.api(hosting).get(&endpoint) else {
            return Vec::new();
        };
        super::parse_commit_avatars(&answer.to_string())
            .into_iter()
            .map(|(email, url)| (email, Hint::Url(url)))
            .collect()
    }

    /// With a login already at hand only: a credential helper may ask for a keyring to be
    /// unlocked, which the inbox, asking unasked, mustn't bring up.
    fn inbox(&self, _: &Path, hosting: &Hosting) -> Result<Answer, String> {
        let login = known_login(&hosting.web);
        let api = Api::new(self.name, &hosting.web, &hosting.host, login)
            .on_refused_key(forget_login, &login_key(&hosting.web));
        inbox::inbox(&api, hosting)
    }

    /// The host's search finds them in every repository there, forks' upstreams too.
    fn inbox_per_host(&self) -> bool {
        true
    }

    fn pages(&self) -> Option<&dyn Pages> {
        Some(self)
    }

    fn forks(&self, _: &Path, hosting: &Hosting) -> Result<Vec<ForkInfo>, String> {
        let endpoint = format!("repos/{}/pulls?state=open", hosting.path);
        let items = self.api(hosting).get_all(&endpoint, 100)?;
        Ok(items.iter().map(super::forks::gitea_one).collect())
    }

    fn create_release(
        &self,
        _: &Path,
        hosting: &Hosting,
        new: &super::NewRelease,
    ) -> Result<String, String> {
        create_release(&self.api(hosting), hosting, new)
    }

    fn upload_assets(
        &self,
        _: &Path,
        hosting: &Hosting,
        tag: &str,
        files: &[PathBuf],
    ) -> Result<(), String> {
        upload_assets(&self.api(hosting), hosting, tag, files)
    }
}

impl Pages for GiteaFamily {
    fn details(&self, _: &Path, hosting: &Hosting, number: u64) -> Result<PrDetails, String> {
        details::read(&self.api(hosting), self.forge, hosting, number)
    }

    fn target(&self, _: &Path, hosting: &Hosting, details: &PrDetails) -> Option<Box<dyn Target>> {
        post::GiteaTarget::of(self.api(hosting), self.name, hosting, details)
            .map(|t| Box::new(t) as Box<dyn Target>)
    }
}
pub(super) fn ssh_key_api(hosting: &Hosting) -> Api {
    Api::new(
        hosting.forge.provider().name(),
        &hosting.web,
        &hosting.host,
        tea_login(&hosting.web),
    )
}

pub(super) fn history_api(hosting: &Hosting) -> Api {
    Api::new(
        "Gitea / Forgejo",
        &hosting.web,
        &hosting.host,
        find_login(&hosting.web),
    )
    .on_refused_key(forget_login, &login_key(&hosting.web))
}

mod login;
#[cfg(test)]
mod tests;
pub(super) use login::tea_servers;
use login::*;
pub(crate) use login::{selected_tea_metadata, selected_tea_secret};
mod pulls;
use pulls::*;
