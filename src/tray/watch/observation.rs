//! Observe forge and branch state through a recording-friendly probe interface.
use super::*;

pub(super) fn transitions(before: &Observation, after: &Observation) -> Vec<Notice> {
    let mut out = Vec::new();
    if let (Some((old_ref, old)), Some((new_ref, new)), Some(n)) =
        (&before.default, &after.default, after.advanced)
        && old_ref == new_ref
        && old != new
        && n > 0
    {
        let branch = new_ref.splitn(4, '/').nth(3).unwrap_or(new_ref);
        out.push(Notice {
            key: format!("repo:advance:{new_ref}:{new}"),
            text: crate::i18n::message(
                "interaction-watch-advance",
                &[("count", n.into()), ("branch", branch.into())],
            ),
        });
    }
    if let (Some(old), Some(new)) = (&before.prs, &after.prs) {
        for pr in new {
            let prior = old.iter().find(|p| p.url == pr.url);
            if pr.review_requested && prior.is_none_or(|p| !p.review_requested) {
                out.push(Notice {
                    key: format!("review:{}:{}", pr.url, pr.updated),
                    text: crate::i18n::message(
                        "interaction-watch-review",
                        &[
                            ("number", pr.number.to_string().into()),
                            ("title", pr.title.as_str().into()),
                        ],
                    ),
                });
            }
            if pr.mine
                && pr.checks == Checks::Failing
                && prior.is_none_or(|p| p.checks != Checks::Failing)
            {
                out.push(Notice {
                    key: format!("checks:{}:{}", pr.url, pr.updated),
                    text: crate::i18n::message(
                        "interaction-watch-checks",
                        &[
                            ("number", pr.number.to_string().into()),
                            ("title", pr.title.as_str().into()),
                        ],
                    ),
                });
            }
        }
    }
    if after.ci_failed == Some(true) && before.ci_failed == Some(false) {
        out.push(Notice {
            key: format!("repo:ci:{:?}", after.default),
            text: crate::i18n::text("interaction-watch-label-10").into(),
        });
    }
    if let (Some(old), Some(new)) = (&before.branch_checks, &after.branch_checks)
        && old.name == new.name
        && new.failed
        && (!old.failed || old.sha != new.sha)
    {
        out.push(Notice {
            key: format!("repo:branch:{}:{}", new.name, new.sha),
            text: crate::i18n::message(
                "interaction-watch-branch-failed",
                &[("branch", new.name.as_str().into())],
            ),
        });
    }
    out
}

pub(super) struct BranchPlan {
    pub(super) name: String,
    pub(super) sha: String,
    pub(super) projects: Vec<crate::ci::Project>,
}

pub(super) fn branch_plan(
    forge: forge::Forge,
    remotes: &[git::Remote],
    refs: &git::Refs,
) -> Option<BranchPlan> {
    let name = refs.current.clone()?;
    if !valid_oid(&refs.head) {
        return None;
    }
    let main = crate::ci::Project::of(forge, remotes)?;
    let mut current = refs.clone();
    current.local.retain(|branch| branch.name == name);
    current.remotes.clear(); // ask this branch's destination(s), not all repository tips
    let plan = crate::ci::route::plan(
        &crate::ci::route::Sources {
            main: &main,
            remotes,
            refs: &current,
            prs: Vec::new(),
            recent: Vec::new(),
        },
        4,
    );
    let projects = plan
        .into_iter()
        .filter(|(_, shas)| shas.contains(&refs.head))
        .map(|(project, _)| project)
        .collect();
    Some(BranchPlan {
        name,
        sha: refs.head.clone(),
        projects,
    })
}

/// All process/provider work is confined to this adapter, invoked by a task, never by a UI
/// callback. The recording adapter exercises the exact orchestration below without runtime.
pub(super) trait Probe {
    fn git(&mut self, args: &[&str]) -> Result<String, String>;
    fn hosting(&mut self) -> Option<forge::Hosting>;
    fn inbox(&mut self, hosting: &forge::Hosting) -> Result<Vec<InboxPr>, String>;
    fn ci_failed(&mut self, hosting: &forge::Hosting, sha: &str) -> Result<Option<bool>, String>;
    fn branch_checks(&mut self, _: &forge::Hosting) -> Result<Option<BranchChecks>, String> {
        Ok(None)
    }
    fn may_run(&self) -> bool;
}

pub(super) fn observe(
    probe: &mut impl Probe,
    before: Option<&Observation>,
) -> Result<Observation, String> {
    if !probe.may_run() {
        return Err(crate::i18n::text("interaction-watch-label-1").into());
    }
    // No transient Credentials object, shell interpolation, askpass or UI authentication.
    probe.git(&["fetch", "--all", "--prune", "--quiet"])?;
    let mut snapshot = Observation::default();
    if !probe.may_run() {
        return Err(crate::i18n::text("interaction-watch-label-2").into());
    }
    let remotes = probe.git(&["remote"])?;
    let remote = remotes
        .lines()
        .find(|r| *r == "origin")
        .or_else(|| remotes.lines().next());
    if let Some(remote) = remote.filter(|r| !r.is_empty() && !r.starts_with('-')) {
        let sym = format!("refs/remotes/{remote}/HEAD");
        let reference = probe.git(&["symbolic-ref", "--quiet", &sym]).or_else(|_| {
            // Repositories created with `git init` + `remote add` need not have a local
            // remote HEAD. Ask the remote's symbolic HEAD without changing checkout/config.
            let answer = probe.git(&["ls-remote", "--symref", "--", remote, "HEAD"])?;
            let branch = answer.lines().find_map(|line| {
                line.strip_prefix("ref: refs/heads/")
                    .and_then(|line| line.strip_suffix("\tHEAD"))
            });
            branch
                .filter(|branch| !branch.is_empty())
                .map(|branch| format!("refs/remotes/{remote}/{branch}"))
                .ok_or_else(|| crate::i18n::text("interaction-watch-label-3").to_owned())
        });
        match reference {
            Ok(reference) => {
                let reference = reference.trim().to_owned();
                let prefix = format!("refs/remotes/{remote}/");
                if reference.starts_with(&prefix) && reference != sym {
                    let object = format!("{reference}^{{commit}}");
                    let sha = probe
                        .git(&["rev-parse", "--verify", "--end-of-options", &object])?
                        .trim()
                        .to_owned();
                    if valid_oid(&sha) {
                        if let Some((old_ref, old_sha)) = before.and_then(|b| b.default.as_ref())
                            && old_ref == &reference
                            && old_sha != &sha
                            && valid_oid(old_sha)
                        {
                            let range = format!("{old_sha}..{sha}");
                            snapshot.advanced = probe
                                .git(&["rev-list", "--count", &range, "--"])
                                .ok()
                                .and_then(|n| n.trim().parse().ok());
                        }
                        snapshot.default = Some((reference, sha));
                    }
                }
            }
            Err(why) => snapshot.warnings.push(crate::i18n::message(
                "interaction-watch-unknown-default",
                &[("detail", why.into())],
            )),
        }
    }
    if probe.may_run()
        && let Some(hosting) = probe.hosting()
    {
        match probe.inbox(&hosting) {
            Ok(prs) => snapshot.prs = Some(prs),
            Err(why) => snapshot.warnings.push(why),
        }
        match probe.branch_checks(&hosting) {
            Ok(checks) => snapshot.branch_checks = checks,
            Err(why) => snapshot.warnings.push(why),
        }
        if let Some((_, sha)) = &snapshot.default {
            match probe.ci_failed(&hosting, sha) {
                Ok(state) => snapshot.ci_failed = state,
                Err(why) => snapshot.warnings.push(why),
            }
        }
    }
    Ok(snapshot)
}

pub(super) fn valid_oid(sha: &str) -> bool {
    matches!(sha.len(), 40 | 64) && sha.bytes().all(|c| c.is_ascii_hexdigit())
}

#[cfg(not(test))]
pub(super) struct LiveProbe<'a> {
    pub(super) path: &'a Path,
    pub(super) cancel: tasks::Cancellation,
}

#[cfg(not(test))]
impl Probe for LiveProbe<'_> {
    fn git(&mut self, args: &[&str]) -> Result<String, String> {
        if !self.may_run() {
            return Err(crate::i18n::text("interaction-watch-label-4").into());
        }
        git::run_unattended_within(self.path, args, STEP_LIMIT)
    }
    fn hosting(&mut self) -> Option<forge::Hosting> {
        let remotes = git::remotes(self.path).ok()?;
        forge::detect(self.path, &remotes)
    }
    fn inbox(&mut self, hosting: &forge::Hosting) -> Result<Vec<InboxPr>, String> {
        if !self.may_run() {
            return Err(crate::i18n::text("interaction-watch-label-4").into());
        }
        hosting
            .forge
            .provider()
            .inbox(self.path, hosting)
            .map(|answer| {
                answer
                    .prs
                    .into_iter()
                    .filter(|pr| {
                        !hosting.forge.provider().inbox_per_host()
                            || pr.project.eq_ignore_ascii_case(&hosting.path)
                    })
                    .collect()
            })
    }
    fn ci_failed(&mut self, hosting: &forge::Hosting, sha: &str) -> Result<Option<bool>, String> {
        if !self.may_run() {
            return Err(crate::i18n::text("interaction-watch-label-4").into());
        }
        let project = crate::ci::Project::of(hosting.forge, &git::remotes(self.path)?);
        let Some(project) = project else {
            return Ok(None);
        };
        crate::ci::statuses(self.path, &project, &[sha.into()]).map(|statuses| {
            statuses
                .get(sha)
                .map(|s| s.state == crate::ci::CheckState::Failed)
        })
    }
    fn branch_checks(&mut self, hosting: &forge::Hosting) -> Result<Option<BranchChecks>, String> {
        if !self.may_run() {
            return Err(crate::i18n::text("interaction-watch-label-4").into());
        }
        let remotes = git::remotes(self.path)?;
        let refs = git::refs(self.path)?;
        let Some(plan) = branch_plan(hosting.forge, &remotes, &refs) else {
            return Ok(None);
        };
        let mut known = false;
        let mut failed = false;
        let mut errors = Vec::new();
        for project in plan.projects {
            if !self.may_run() {
                return Err(crate::i18n::text("interaction-watch-label-4").into());
            }
            match crate::ci::statuses(self.path, &project, std::slice::from_ref(&plan.sha)) {
                Ok(statuses) => {
                    if let Some(status) = statuses.get(&plan.sha) {
                        known = true;
                        failed |= status.state == crate::ci::CheckState::Failed;
                    }
                }
                Err(why) => errors.push(why),
            }
        }
        if !known && !errors.is_empty() {
            return Err(errors.join("; "));
        }
        Ok(known.then_some(BranchChecks {
            name: plan.name,
            sha: plan.sha,
            failed,
        }))
    }
    fn may_run(&self) -> bool {
        !self.cancel.is_cancelled() && crate::network::background_may_run()
    }
}
