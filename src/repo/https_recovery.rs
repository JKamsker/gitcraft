//! Bind sign-in workflows to the tab, connection identity and submitted operation.
use super::{Dialog, RepoTab};
use crate::git;
#[derive(Clone)]
pub(super) struct Ticket {
    pub(super) url: String,
    pub(super) endpoints: Vec<String>,
    pub(super) binding: Option<git::RouteBinding>,
    generation: u64,
    path: std::path::PathBuf,
    context: crate::connections::Context,
    remotes: Vec<git::Remote>,
}
impl Ticket {
    pub(super) fn authorizes_endpoint(&self, url: &str) -> bool {
        self.binding
            .as_ref()
            .is_some_and(|binding| binding.authorizes_endpoint(url))
            && git::CredentialScope::for_url(url, "endpoint-check")
                .is_ok_and(|scope| self.endpoints.iter().any(|e| scope.matches_url(e)))
    }
}
impl RepoTab {
    pub(super) fn inspect_unbound_https(&mut self, urls: Vec<String>, ctx: &egui::Context) {
        self.invalidate_https_challenge();
        self.status = Some(crate::repo::Notice::plain((
            crate::i18n::text("repo-core-2803").into(),
            true,
        )));
        crate::settings::network_page::inspect_secondary(ctx, self.path.clone(), urls);
    }
    pub(super) fn https_ticket(&self, url: String) -> Ticket {
        Ticket {
            endpoints: vec![url.clone()],
            binding: None,
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
        self.https_checking = false;
        if matches!(self.dialog, Some(Dialog::Credentials { .. })) {
            self.dialog = None;
        }
    }
    pub(super) fn offer_https_signin(&mut self, url: String, refused: bool) {
        self.https_challenge = Some(self.https_ticket(url.clone()));
        self.dialog = Some(Dialog::credentials(&url, refused));
    }
    pub(super) fn offer_failed_https_signin(
        &mut self,
        mut failure: git::AuthFailure,
        refused: bool,
        ctx: &egui::Context,
    ) {
        if failure.context != self.connection_context
            || failure.context != crate::connections::current()
        {
            self.status = Some(crate::repo::Notice::plain((
                crate::i18n::text("repo-core-2804").into(),
                true,
            )));
            return;
        }
        if failure.needs_separate_account() {
            self.invalidate_https_challenge();
            self.status = Some(crate::repo::Notice::plain((
                crate::i18n::text("repo-core-2805").into(),
                true,
            )));
            crate::settings::network_page::inspect_secondary(
                ctx,
                self.path.clone(),
                failure.endpoints,
            );
            return;
        }
        if failure.binding.is_none() {
            self.invalidate_https_challenge();
            self.status = Some(crate::repo::Notice::plain((
                crate::i18n::text("repo-core-2806").into(),
                true,
            )));
            return;
        }
        let (bound, foreign): (Vec<_>, Vec<_>) = failure.endpoints.into_iter().partition(|url| {
            failure
                .binding
                .as_ref()
                .is_some_and(|b| b.authorizes_endpoint(url))
        });
        failure.endpoints = bound;
        if !foreign.is_empty() {
            crate::settings::network_page::inspect_secondary(ctx, self.path.clone(), foreign);
        }
        if failure.endpoints.is_empty() {
            self.status = Some(crate::repo::Notice::plain((
                crate::i18n::text("repo-core-2807").into(),
                true,
            )));
            return;
        }
        let url = if failure.confirmed {
            failure.endpoints[0].clone()
        } else {
            String::new()
        };
        let mut ticket = self.https_ticket(url.clone());
        ticket.endpoints = failure.endpoints.clone();
        ticket.binding = failure.binding;
        let mut dialog = Dialog::credentials(&url, refused);
        if let Dialog::Credentials { endpoints, .. } = &mut dialog {
            *endpoints = failure.endpoints;
        }
        self.https_challenge = Some(ticket);
        self.dialog = Some(dialog);
    }
    pub(super) fn https_rejection_done(&mut self, ticket: Ticket, result: Result<(), String>) {
        if !self.https_ticket_current(&ticket) || !ticket.authorizes_endpoint(&ticket.url) {
            self.finished.push(crate::repo::Notice::plain((
                crate::i18n::message(
                    "repo-core-2808",
                    &[(
                        "failed",
                        (if result.is_ok() {
                            crate::i18n::text("repo-core-2809")
                        } else {
                            crate::i18n::text("repo-core-0290")
                        })
                        .to_string()
                        .into(),
                    )],
                ),
                result.is_err(),
            )));
            return;
        }
        match result {
            Ok(()) => {
                self.status = Some(crate::repo::Notice::plain((
                    crate::i18n::text("repo-core-2810").into(),
                    false,
                )));
                let url = ticket.url.clone();
                self.offer_https_signin(url, false);
                if let Some(current) = &mut self.https_challenge {
                    current.binding = ticket.binding;
                }
            }
            Err(e) => {
                self.status = Some(crate::repo::Notice::plain((
                    crate::i18n::message(
                        "repo-core-2811",
                        &[("e", crate::diagnostics::crash::scrub(&e).to_string().into())],
                    ),
                    true,
                )))
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_route_proof_cannot_schedule_signin_rejection_or_retry() -> Result<(), String> {
        let ctx = egui::Context::default();
        crate::connections::Context::default().run(|| -> Result<(), String> {
            let mut tab = RepoTab::new_deferred("/nonexistent/missing-route".into(), &ctx);
            let failure = git::AuthFailure::from_error(
                "fatal: Authentication failed for 'https://push.invalid/repo'",
                &[],
            );
            tab.offer_failed_https_signin(failure, false, &ctx);
            assert!(tab.dialog.is_none() && tab.https_challenge.is_none());
            tab.switch_https("https://push.invalid/repo".into(), &ctx);
            tab.sign_in(
                git::Credentials::for_url(
                    "https://push.invalid/repo",
                    "alice".into(),
                    "DUMMY".into(),
                )?,
                &ctx,
            );
            assert!(!tab.https_checking && tab.busy_op.is_none() && tab.running.is_none());
            assert!(tab.credentials.is_none());
            assert!(crate::settings::network_page::take_rejections(&ctx).is_empty());
            assert!(!crate::settings::network_page::take_inspection_request(
                &ctx
            ));
            Ok(())
        })
    }
    #[test]
    fn lfs_recovery_stages_only_standalone_endpoint_inspection() {
        let ctx = egui::Context::default();
        crate::connections::Context::default().run(|| {
            let mut tab = RepoTab::new_deferred("/nonexistent/lfs-route".into(), &ctx);
            let failure = git::AuthFailure::from_error(
                "LFS: https://lfs.invalid/repo/info/lfs authentication required",
                &[],
            );
            tab.offer_failed_https_signin(failure, false, &ctx);
            assert!(tab.dialog.is_none() && tab.https_challenge.is_none());
            assert!(!tab.https_checking && tab.busy_op.is_none() && tab.running.is_none());
            assert!(crate::settings::network_page::take_rejections(&ctx).is_empty());
            assert!(crate::settings::network_page::take_inspection_request(&ctx));
        });
    }
    fn recorded_binding() -> Result<git::RouteBinding, String> {
        serde_json::from_value(serde_json::json!({
            "args": ["push", "origin"],
            "routes": ["https://git.invalid/repo", "https://mirror.invalid/repo"],
            "configuration": "remote.origin.url\nhttps://git.invalid/repo\u{0}"
        }))
        .map_err(|e| e.to_string())
    }
    #[test]
    fn mixed_failures_keep_bound_choices_and_separate_independent_routes() -> Result<(), String> {
        crate::connections::Context::default().run(|| -> Result<(), String> {
            // An independent route can move while ordinary Git proof is unchanged.
            // Neither its original nor edited URL acquires ordinary-route authority.
            let binding = recorded_binding()?;
            for foreign in ["https://lfs.invalid/old", "https://other-lfs.invalid/new"] {
                let ctx = egui::Context::default();
                let mut tab = RepoTab::new_deferred("/nonexistent/mixed-route".into(), &ctx);
                let error = format!(
                    "fatal: Authentication failed for 'https://git.invalid/repo'\nfatal: Authentication failed for 'https://mirror.invalid/repo'\nfatal: Authentication failed for '{foreign}'"
                );
                let mut failure = git::AuthFailure::from_error(&error, &[]);
                assert!(!failure.confirmed);
                failure.binding = Some(binding.clone());
                tab.offer_failed_https_signin(failure, true, &ctx);
                let ticket = tab.https_challenge.as_ref().ok_or("Missing normal choices")?;
                assert!(ticket.url.is_empty());
                assert_eq!(ticket.endpoints, ["https://git.invalid/repo", "https://mirror.invalid/repo"]);
                assert!(ticket.authorizes_endpoint("https://git.invalid/repo/"));
                assert!(ticket.authorizes_endpoint("https://mirror.invalid/repo"));
                assert!(!ticket.authorizes_endpoint(foreign));
                assert!(matches!(&tab.dialog, Some(Dialog::Credentials{endpoints,..}) if endpoints == &ticket.endpoints));
                assert!(crate::settings::network_page::take_inspection_request(&ctx));
                assert!(crate::settings::network_page::take_rejections(&ctx).is_empty());
                assert!(!tab.https_checking && tab.running.is_none() && tab.busy_op.is_none());
            }
            Ok(())
        })
    }
    #[test]
    fn foreign_selection_cannot_sign_in_or_forget_even_if_ticket_lists_it() -> Result<(), String> {
        crate::connections::Context::default().run(|| -> Result<(), String> {
            for sign_in in [true, false] {
                let ctx = egui::Context::default();
                let mut tab = RepoTab::new_deferred("/nonexistent/foreign-choice".into(), &ctx);
                let foreign = "https://secondary.invalid/repo";
                let mut ticket = tab.https_ticket(foreign.into());
                ticket.endpoints.push("https://git.invalid/repo".into());
                ticket.binding = Some(recorded_binding()?);
                assert!(!ticket.authorizes_endpoint(foreign));
                tab.https_challenge = Some(ticket);
                tab.credentials = Some(git::Credentials::for_url(
                    "https://git.invalid/repo",
                    "alice".into(),
                    "DUMMY_RETAINED".into(),
                )?);
                if sign_in {
                    tab.sign_in(
                        git::Credentials::for_url(foreign, "bob".into(), "DUMMY_FOREIGN".into())?,
                        &ctx,
                    );
                } else {
                    tab.switch_https(foreign.into(), &ctx);
                }
                assert_eq!(
                    tab.credentials.as_ref().map(|c| c.username.as_str()),
                    Some("alice")
                );
                assert!(!tab.https_checking && tab.running.is_none() && tab.busy_op.is_none());
                assert!(tab.dialog.is_none() && tab.https_challenge.is_none());
                assert!(crate::settings::network_page::take_rejections(&ctx).is_empty());
                assert!(crate::settings::network_page::take_inspection_request(&ctx));
            }
            Ok(())
        })
    }
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
