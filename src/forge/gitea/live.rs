//! The whole of a pull request's life against real servers: a Forgejo and a Gitea started by
//! `ci/forge-server.sh`, which says how to run these. They're ignored by a plain `cargo test`:
//!
//! ```sh
//! ci/forge-server.sh forgejo 3000 forges.env && ci/forge-server.sh gitea 3001 forges.env
//! set -a; . ./forges.env; set +a
//! cargo test live -- --ignored
//! ```
//!
//! Each run makes a new repository on the server, so a server can be used again.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::json;

use super::details::{self, thread_id};
use super::post::GiteaTarget;
use super::{
    Api, FORGEJO, GITEA, GiteaFamily, Login, create, create_release, list, retarget, upload_assets,
};
use crate::forge::{self, Forge, Found, Hosting, NewPullRequest, Provider};
use crate::git;
use crate::pr_post::{self, LineComment, MergeMethod, Suggestion, Target, Verdict};
use crate::pull_request::{self, DiffSide, EntryKind, PrDetails, at};

macro_rules! ensure {
    ($cond:expr, $($why:tt)+) => {
        if !$cond {
            return Err(format!($($why)+));
        }
    };
}

#[test]
#[ignore = "needs a Forgejo: see ci/forge-server.sh"]
fn live_forgejo() -> Result<(), String> {
    whole_life(&FORGEJO)
}

#[test]
#[ignore = "needs a Gitea: see ci/forge-server.sh"]
fn live_gitea() -> Result<(), String> {
    whole_life(&GITEA)
}

/// A server and its two users.
pub(super) struct Server {
    family: &'static GiteaFamily,
    /// `http://127.0.0.1:3000`.
    pub(super) url: String,
    pub(super) author_token: String,
    reviewer_token: String,
}

impl Server {
    pub(super) fn from_env(family: &'static GiteaFamily) -> Result<Server, String> {
        let upper = family.name.to_uppercase();
        let var = |name: &str| {
            let key = format!("GITCRAFT_{upper}_{name}");
            std::env::var(&key).map_err(|_| {
                format!("{key} isn't set: start a server with ci/forge-server.sh (see its header)")
            })
        };
        Ok(Server {
            family,
            url: var("URL")?.trim_end_matches('/').to_owned(),
            author_token: var("AUTHOR_TOKEN")?,
            reviewer_token: var("REVIEWER_TOKEN")?,
        })
    }

    fn api(&self, token: Option<&str>) -> Api {
        let login = token.map(|t| Login::Token(t.to_owned()));
        Api::new(self.family.name, &self.url, "127.0.0.1", login)
    }

    pub(super) fn author(&self) -> Api {
        self.api(Some(&self.author_token))
    }

    pub(super) fn reviewer(&self) -> Api {
        self.api(Some(&self.reviewer_token))
    }
}

/// Polls `f` until it has something, for what the server works out in the background.
fn eventually<T>(
    what: &str,
    mut f: impl FnMut() -> Result<Option<T>, String>,
) -> Result<T, String> {
    let start = Instant::now();
    loop {
        if let Some(found) = f()? {
            return Ok(found);
        }
        if start.elapsed() > Duration::from_secs(60) {
            return Err(format!("timed out waiting for {what}"));
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

pub(super) fn run(dir: &Path, args: &[&str]) -> Result<String, String> {
    git::run(Some(dir), args).map_err(|e| format!("git {}: {e}", args.join(" ")))
}

pub(super) fn write(dir: &Path, file: &str, text: &str) -> Result<(), String> {
    std::fs::write(dir.join(file), text).map_err(|e| e.to_string())
}

pub(super) fn scratch(name: &str) -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

mod lifecycle;
mod publication;
mod reviews;
use lifecycle::whole_life;
use publication::*;
