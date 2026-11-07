//! Captured Git destinations and the lifetime of an explicit fork action.
use std::path::Path;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

use serde::{Deserialize, Serialize};

use crate::{forge::Hosting, git};

#[derive(Clone)]
pub(in crate::repo) struct Scope {
    pub generation: u64,
    pub id: u64,
    pub hosting: Hosting,
    pub primary: git::Remote,
    valid: Arc<AtomicBool>,
    fork: Option<(String, String, Hosting, bool)>,
}
static NEXT_SCOPE: AtomicU64 = AtomicU64::new(1);
impl Scope {
    pub fn new(generation: u64, hosting: Hosting, primary: git::Remote) -> Self {
        Self {
            generation,
            id: NEXT_SCOPE.fetch_add(1, Ordering::Relaxed),
            hosting,
            primary,
            valid: Arc::new(AtomicBool::new(true)),
            fork: None,
        }
    }
    pub fn with_fork(mut self, name: String, url: String, hosting: Hosting, new: bool) -> Self {
        self.fork = Some((name, url, hosting, new));
        self
    }
    pub fn invalidate(&self) {
        self.valid.store(false, Ordering::SeqCst);
    }
    pub fn check_remote(&self, repo: &Path) -> Result<(), String> {
        self.check()?;
        if !git::remotes(repo)?.iter().any(|r| r == &self.primary) {
            return Err(
                "The primary remote changed or was removed; plan the fork action again".into(),
            );
        }
        let destination = Destination::read(repo, &self.primary.name)?;
        if destination.fetch.first() != Some(&self.primary.url) {
            return Err(
                "The effective primary fetch URL changed; plan the fork action again".into(),
            );
        }
        if let Some((name, url, hosting, new)) = &self.fork {
            let present = git::remotes(repo)?.iter().any(|r| &r.name == name);
            if present || !new {
                let destination = Destination::read(repo, name)?;
                if destination.fetch.first() != Some(url) {
                    return Err(
                        "The contributor's fork remote changed; check out the pull request again"
                            .into(),
                    );
                }
                destination.validate(Some(hosting), false)?;
                destination.validate(Some(hosting), true)?;
            }
        }
        Ok(())
    }
    pub fn check(&self) -> Result<(), String> {
        if self.valid.load(Ordering::SeqCst) {
            Ok(())
        } else {
            Err("The primary forge changed; plan this fork action again. Any completed local rewrite remains undoable.".into())
        }
    }
}

/// Git's effective URLs, including insteadOf, pushInsteadOf and pushurl. Keep the
/// catalog name too: removal/rename requires a fresh plan even for the same repository.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Destination {
    pub name: String,
    pub fetch: Vec<String>,
    pub push: Vec<String>,
}
impl Destination {
    pub fn read(repo: &Path, name: &str) -> Result<Self, String> {
        let read = |push| {
            let mut args = vec!["remote", "get-url", "--all"];
            if push {
                args.push("--push");
            }
            args.extend(["--", name]);
            git::run(Some(repo), &args)
                .map(|out| out.lines().map(str::to_owned).collect::<Vec<_>>())
        };
        Ok(Self {
            name: name.into(),
            fetch: read(false)?,
            push: read(true)?,
        })
    }
    pub fn validate(&self, hosting: Option<&Hosting>, push: bool) -> Result<(), String> {
        let urls = if push { &self.push } else { &self.fetch };
        if urls.is_empty() || hosting.is_some_and(|h| !urls.iter().all(|u| h.matches_git_remote(u)))
        {
            return Err(format!(
                "{} no longer has matching {} destination for this forge; plan the fork action again",
                self.name,
                if push { "push" } else { "fetch" }
            ));
        }
        Ok(())
    }
    pub fn step(&self) -> Vec<String> {
        vec![MARK.into(), serde_json::to_string(self).unwrap_or_default()]
    }
}
const MARK: &str = "\0fork-destination";
pub(in crate::repo) fn is_guard(step: &[String]) -> bool {
    matches!(step, [mark, _] if mark == MARK)
}
/// Rechecked before every real step, including immediately before a force push. These
/// checks also survive retry and persisted undo/redo; no Git command receives the marker.
pub(in crate::repo) fn check(repo: &Path, steps: &[Vec<String>]) -> Result<(), String> {
    for step in steps.iter().filter(|s| is_guard(s)) {
        let expected: Destination = serde_json::from_str(&step[1]).map_err(|e| e.to_string())?;
        if Destination::read(repo, &expected.name).as_ref() != Ok(&expected) {
            return Err(format!(
                "Remote {} changed or was removed; plan the fork action again",
                expected.name
            ));
        }
    }
    Ok(())
}
