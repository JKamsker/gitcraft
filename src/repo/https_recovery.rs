//! Bind sign-in workflows to the tab, connection identity and submitted operation.
use super::{Dialog, RepoTab};
use crate::git;
#[derive(Clone)]
pub(super) struct Ticket {
    pub(super) url: String,
    pub(super) endpoints: Vec<String>,
    generation: u64,
    path: std::path::PathBuf,
    context: crate::connections::Context,
    remotes: Vec<git::Remote>,
}
impl RepoTab {
    pub(super) fn https_ticket(&self, url: String) -> Ticket {
        Ticket {
            endpoints: vec![url.clone()],
            url,
            generation: self.https_generation,
            path: self.path.clone(),
            context: crate::connections::current(),
            remotes: self.remotes.clone(),
        }
    }
    pub(super) fn https_ticket_current(&self, ticket: &Ticket) -> bool {
        ticket.generation == self.https_generation
            && ticket.path == self.path
            && ticket.context == self.connection_context
            && ticket.context == crate::connections::current()
            && ticket.remotes == self.remotes
    }
    pub(super) fn invalidate_https_challenge(&mut self) {
        self.https_generation = self.https_generation.wrapping_add(1);
        self.https_challenge = None;
        if matches!(self.dialog, Some(Dialog::Credentials { .. })) {
            self.dialog = None;
        }
    }
    pub(super) fn offer_https_signin(&mut self, url: String, refused: bool) {
        self.https_challenge = Some(self.https_ticket(url.clone()));
        self.dialog = Some(Dialog::credentials(&url, refused));
    }
    pub(super) fn offer_failed_https_signin(&mut self, failure: git::AuthFailure, refused: bool) {
        if failure.context != self.connection_context
            || failure.context != crate::connections::current()
        {
            self.status = Some(("The operation used a previous connection. Run it again with the current profile before signing in.".into(), true));
            return;
        }
        if failure.endpoints.is_empty() {
            self.status = Some(("Git did not report an HTTP(S) sign-in endpoint. Inspect HTTPS sign-ins in Settings, then run the operation again.".into(), true));
            return;
        }
        let url = if failure.confirmed {
            failure.endpoints[0].clone()
        } else {
            String::new()
        };
        let mut ticket = self.https_ticket(url.clone());
        ticket.endpoints = failure.endpoints.clone();
        let mut dialog = Dialog::credentials(&url, refused);
        if let Dialog::Credentials { endpoints, .. } = &mut dialog {
            *endpoints = failure.endpoints;
        }
        self.https_challenge = Some(ticket);
        self.dialog = Some(dialog);
    }
    pub(super) fn https_rejection_done(&mut self, ticket: Ticket, result: Result<(), String>) {
        if !self.https_ticket_current(&ticket) {
            self.finished.push((format!("Sign-in rejection for the previous connection {}. Current sign-in was not changed.", if result.is_ok() { "finished" } else { "failed" }), result.is_err()));
            return;
        }
        match result {
            Ok(()) => {
                self.status = Some(("Stored HTTPS sign-in rejected. Choose the correct account; profile-selected forge accounts can be changed in Settings → Profile.".into(), false));
                self.offer_https_signin(ticket.url, false);
            }
            Err(e) => {
                self.status = Some((
                    format!(
                        "Could not forget the stored sign-in: {}",
                        crate::diagnostics::crash::scrub(&e)
                    ),
                    true,
                ))
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn connection_or_operation_changes_discard_old_rejection_completion_and_retry() {
        let ctx = egui::Context::default();
        let original = crate::connections::Context::default();
        original.run(|| {
            let mut tab = RepoTab::new_deferred("/nonexistent/https-generation".into(), &ctx);
            let ticket = tab.https_ticket("https://fixture.invalid/old-account".into());
            tab.invalidate_https_challenge();
            tab.https_rejection_done(ticket, Ok(()));
            assert!(tab.dialog.is_none());
            assert!(tab.https_challenge.is_none());
            let ticket = tab.https_ticket("https://fixture.invalid/old-account".into());
            let edited = crate::connections::Context {
                error: Some("edited fixture account".into()),
                ..Default::default()
            };
            edited.run(|| {
                tab.notice_connections(&ctx);
                tab.https_rejection_done(ticket, Ok(()));
            });
            assert!(tab.dialog.is_none());
            assert!(tab.https_challenge.is_none());
            let ticket = tab.https_ticket("https://fixture.invalid/old-account".into());
            tab.path = "/nonexistent/replaced-tab".into();
            assert!(!tab.https_ticket_current(&ticket));
        });
    }
}
