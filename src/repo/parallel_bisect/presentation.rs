use super::*;

pub(in crate::repo) fn completion_notice(
    outcome: &Outcome,
    machine: &Machine,
    paint: &PresentationState,
) -> super::super::Notice {
    super::super::Notice::parallel(outcome, machine, paint.failed.clone())
}

/// How a parallel bisect ended, for the status line: the message, and whether it failed.
pub(in crate::repo) fn outcome_message(outcome: &Outcome, machine: &str) -> (String, bool) {
    match outcome {
        Outcome::FirstBad(hash) => (
            format!("Parallel bisect: {} is the first bad commit", short(hash)),
            false,
        ),
        Outcome::OneOf(hashes) => (
            format!(
                "Parallel bisect: only skipped commits are left, the first bad commit is one of \
                 these {}",
                hashes.len()
            ),
            false,
        ),
        Outcome::GoodEndFails { hash, .. } => (
            format!(
                "Parallel bisect: the good end already fails ({} fails the test on {machine})",
                short(hash)
            ),
            true,
        ),
        Outcome::BadEndPasses { hash, .. } => (
            format!(
                "Parallel bisect: the bad end passes ({} passes the test on {machine})",
                short(hash)
            ),
            true,
        ),
        Outcome::Cancelled => ("Parallel bisect cancelled".to_owned(), false),
        // Its first line: what follows (a test's output) shows on the banner's hover.
        Outcome::Failed(e) => (
            format!(
                "Parallel bisect failed: {}",
                e.lines().next().unwrap_or_default()
            ),
            true,
        ),
    }
}

/// The machines a bisect can run on, for the repository at `repo`: its own first, then this
/// computer, the SSH hosts of `~/.ssh/config` (WSL distributions come later, looked up in the
/// background).
pub(in crate::repo) fn machines(repo: &std::path::Path) -> Vec<Machine> {
    let mut list = vec![Machine::of(repo)];
    let mut add = |m: Machine| {
        if !list.contains(&m) {
            list.push(m);
        }
    };
    add(Machine::Here);
    for host in crate::remote::ssh_config_hosts() {
        add(Machine::There(crate::remote::Host::Ssh(host)));
    }
    list
}

#[derive(Default)]
pub(super) struct PresentationState {
    pub(super) failed: Option<pb::presentation::Presentation>,
    pub(super) warning: Option<pb::presentation::CleanupWarning>,
}
impl PresentationState {
    pub(in crate::repo) fn take(&mut self, event: Event) -> Event {
        match event {
            Event::DoneWithPresentation(outcome, presentation) => {
                self.failed = match &outcome {
                    Outcome::Failed(raw) => presentation.and_then(|p| p.matching(raw)),
                    _ => None,
                };
                Event::Done(outcome)
            }
            Event::WarningWithPresentation(raw, presentation) => {
                self.warning = Some(presentation);
                Event::Warning(raw)
            }
            Event::Done(outcome) => {
                self.failed = None;
                Event::Done(outcome)
            }
            Event::Warning(raw) => {
                self.warning = None;
                Event::Warning(raw)
            }
            other => other,
        }
    }
    pub(in crate::repo) fn error(&self, raw: &str) -> String {
        pb::presentation::error(raw, self.failed.as_ref())
    }
    pub(in crate::repo) fn warning(&self, raw: &str) -> String {
        self.warning
            .as_ref()
            .map_or_else(|| raw.into(), |p| p.render(raw))
    }
}
