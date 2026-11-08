//! Captured Git destinations and the lifetime of an explicit fork action.
use std::path::Path;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

use serde::{Deserialize, Serialize};

use crate::{
    forge::{Forge, Hosting},
    git,
};

#[derive(Clone)]
pub(in crate::repo) struct Scope {
    pub generation: u64,
    pub id: u64,
    pub hosting: Hosting,
    pub primary: git::Remote,
    valid: Arc<AtomicBool>,
    preference: Option<(Arc<AtomicU64>, u64)>,
    fork: Option<Arc<Mutex<ForkDestination>>>,
    rewrite: Arc<Mutex<Option<Rewrite>>>,
}
struct ForkDestination {
    name: String,
    url: String,
    hosting: Hosting,
    new: bool,
    initial: Option<Destination>,
    seen: bool,
}

/// Evidence from the worker that actually invoked rebase, never inferred at completion.
#[derive(Clone)]
pub(in crate::repo) struct Rewrite {
    pub branch: String,
    pub before: String,
    pub tip: String,
    pub conflicts: bool,
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
            rewrite: Arc::new(Mutex::new(None)),
            preference: None,
        }
    }
    pub fn with_forge_lifetime(mut self, ctx: &egui::Context, repo: &Path) -> Self {
        self.preference = Some(crate::repo::forge_selection::lifetime(ctx, repo));
        self
    }
    pub fn with_fork(mut self, name: String, url: String, hosting: Hosting, new: bool) -> Self {
        self.fork = Some(Arc::new(Mutex::new(ForkDestination {
            name,
            url,
            hosting,
            new,
            initial: None,
            seen: false,
        })));
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
        if let Some(fork) = &self.fork {
            let mut fork = fork.lock().unwrap_or_else(|e| e.into_inner());
            let present = git::remotes(repo)?.iter().any(|r| r.name == fork.name);
            if !present && (!fork.new || fork.seen) {
                return Err(
                    "The contributor's fork remote was removed; plan the action again".into(),
                );
            }
            if present {
                let destination = Destination::read(repo, &fork.name)?;
                if let Some(initial) = &fork.initial {
                    if &destination != initial {
                        return Err(
                            "The contributor's fork remote changed; plan the action again".into(),
                        );
                    }
                } else {
                    // A new remote has just been added from the advertised URL. Git now
                    // supplies its effective fetch AND push URLs, including rewrite rules.
                    if !fork.new && destination.fetch.first() != Some(&fork.url) {
                        return Err(
                            "The contributor's fork remote changed; plan the action again".into(),
                        );
                    }
                    destination.validate(Some(&fork.hosting), false)?;
                    destination.validate(Some(&fork.hosting), true)?;
                    fork.initial = Some(destination);
                }
                fork.seen = true;
            }
        }
        self.check()
    }
    pub fn reset_rewrite(&self) {
        *self.rewrite.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
    pub fn record_rewrite(&self, rewrite: Rewrite) {
        *self.rewrite.lock().unwrap_or_else(|e| e.into_inner()) = Some(rewrite);
    }
    pub fn rewrite(&self) -> Option<Rewrite> {
        self.rewrite
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
    pub fn check(&self) -> Result<(), String> {
        if self.valid.load(Ordering::SeqCst)
            && self
                .preference
                .as_ref()
                .is_none_or(|(clock, read)| clock.load(Ordering::SeqCst) == *read)
        {
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
    matches!(step, [mark, _] if mark == MARK || mark == CHECKOUT_MARK)
}
/// Rechecked before every real step, including immediately before a force push. These
/// checks also survive retry and persisted undo/redo; no Git command receives the marker.
pub(in crate::repo) fn check(repo: &Path, steps: &[Vec<String>]) -> Result<(), String> {
    for step in steps
        .iter()
        .filter(|s| matches!(s.as_slice(), [mark, _] if mark == MARK))
    {
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

/// Unlike a Scope, this guard travels in the persisted checkout undo/redo commands.
/// A removed new remote may be recreated only by its captured remote-add step.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct CheckoutDestination {
    name: String,
    advertised: String,
    new: bool,
    forge: String,
    host: String,
    web: String,
    path: String,
    initial: Option<Destination>,
}
const CHECKOUT_MARK: &str = "\0fork-checkout";
impl CheckoutDestination {
    pub fn new(name: &str, advertised: &str, new: bool, hosting: &Hosting) -> Self {
        Self {
            name: name.into(),
            advertised: advertised.into(),
            new,
            forge: hosting.forge.key().into(),
            host: hosting.host.clone(),
            web: hosting.web.clone(),
            path: hosting.path.clone(),
            initial: None,
        }
    }
    pub fn step(&self) -> Vec<String> {
        vec![
            CHECKOUT_MARK.into(),
            serde_json::to_string(self).unwrap_or_default(),
        ]
    }
    fn hosting(&self) -> Result<Hosting, String> {
        Ok(Hosting {
            forge: Forge::from_key(&self.forge).ok_or("Unknown saved checkout forge")?,
            host: self.host.clone(),
            web: self.web.clone(),
            path: self.path.clone(),
        })
    }
}

pub(in crate::repo) struct CheckoutReplay {
    destination: CheckoutDestination,
    may_create: bool,
}
impl CheckoutReplay {
    pub fn read(ops: &[Vec<String>], saved_label: Option<&str>) -> Result<Option<Self>, String> {
        let Some(step) = ops
            .iter()
            .find(|s| matches!(s.as_slice(), [mark, _] if mark == CHECKOUT_MARK))
        else {
            // Pre-guard timeline entries have the checkout action's original label but
            // lack the identity and effective URLs needed to replay it safely.
            let legacy = saved_label
                .and_then(|s| s.strip_prefix("Check out #"))
                .and_then(|s| s.split_once(" to push to "))
                .is_some_and(|(number, _)| number.parse::<u64>().is_ok());
            return if legacy {
                Err("This saved fork checkout has no captured destination; check out the pull request again".into())
            } else {
                Ok(None)
            };
        };
        let destination: CheckoutDestination =
            serde_json::from_str(&step[1]).map_err(|e| e.to_string())?;
        if saved_label.is_some() && destination.initial.is_none() {
            return Err("The saved fork checkout has no initial effective destination; check out the pull request again".into());
        }
        let may_create = destination.new && ops.iter().any(|s| matches!(s.as_slice(), [remote, add, dash, name, url]
            if remote == "remote" && add == "add" && dash == "--" && name == &destination.name && url == &destination.advertised));
        Ok(Some(Self {
            destination,
            may_create,
        }))
    }
    pub fn check(&mut self, repo: &Path) -> Result<(), String> {
        let d = &mut self.destination;
        let present = git::remotes(repo)?.iter().any(|r| r.name == d.name);
        if !present {
            return if self.may_create {
                Ok(())
            } else {
                Err("The saved fork remote is missing; check out the pull request again".into())
            };
        }
        self.may_create = false;
        let now = Destination::read(repo, &d.name)?;
        let hosting = d.hosting()?;
        now.validate(Some(&hosting), false)?;
        now.validate(Some(&hosting), true)?;
        if let Some(initial) = &d.initial {
            if &now != initial {
                return Err(
                    "The saved fork destination changed; check out the pull request again".into(),
                );
            }
        } else {
            d.initial = Some(now);
        }
        Ok(())
    }
    pub fn bind(&self, ops: &mut [Vec<String>]) {
        for step in ops {
            if matches!(step.as_slice(), [mark, _] if mark == CHECKOUT_MARK) {
                *step = self.destination.step();
            }
        }
    }
}

/// Filled only from the source OID of a push this worker completed. Persisting the
/// explicit OID keeps undo/redo independent of remote fetch-refspec configuration.
pub(super) const PUSHED_TIP: &str = "\0fork-pushed-tip";

pub(in crate::repo) struct Pushed {
    remote: String,
    target: String,
    tip: String,
}
pub(in crate::repo) fn prepare_push(
    repo: &Path,
    op: &[String],
    guarded: bool,
    rewrite: Option<&Rewrite>,
) -> Result<Option<(Vec<String>, Pushed)>, String> {
    if !guarded {
        return Ok(None);
    }
    let Some((lease_at, remote, refspec)) = push_parts(op) else {
        return Ok(None);
    };
    let lease = &op[lease_at];
    let Some((source, target)) = refspec.split_once(':') else {
        return Err("Missing fork push destination".into());
    };
    let Some(expected) = lease.strip_prefix(&format!("--force-with-lease={target}:")) else {
        return Err("Fork push requires an explicit expected commit".into());
    };
    if expected.is_empty() || expected.contains('\0') {
        return Err("Fork push has no recorded expected commit; plan the action again".into());
    }
    let tip = git::resolve(repo, source)?;
    if let Some(rewrite) = rewrite
        && source == rewrite.branch
        && tip != rewrite.tip
    {
        return Err("The rebased branch changed before its push; plan the action again".into());
    }
    let mut args = op.to_vec();
    *args.last_mut().ok_or("Missing fork push refspec")? = format!("{tip}:{target}");
    Ok(Some((
        args,
        Pushed {
            remote: remote.clone(),
            target: target.into(),
            tip,
        },
    )))
}
// Optional push flags (for example --no-verify) must not disable source/lease capture.
fn push_parts(op: &[String]) -> Option<(usize, &String, &String)> {
    if op.first().is_none_or(|s| s != "push") {
        return None;
    }
    let dash = op.iter().position(|s| s == "--")?;
    if op.len() != dash + 3 {
        return None;
    }
    let lease = (1..dash).find(|i| op[*i].starts_with("--force-with-lease="))?;
    Some((lease, &op[dash + 1], &op[dash + 2]))
}
pub(in crate::repo) fn bind_leases(ops: &mut [Vec<String>], pushed: &[Pushed]) {
    for op in ops {
        if let Some((lease, remote, refspec)) = push_parts(op)
            && let Some((_, target)) = refspec.split_once(':')
            && let Some(done) = pushed
                .iter()
                .rev()
                .find(|p| p.remote == *remote && p.target == target)
            && op[lease].starts_with(&format!("--force-with-lease={target}:"))
        {
            op[lease] = format!("--force-with-lease={target}:{}", done.tip);
        }
    }
}
