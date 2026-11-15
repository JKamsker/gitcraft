use super::*;
use crate::forge::discovery::Visibility;
fn text(key: &'static str) -> &'static str {
    crate::i18n::text(key)
}
fn error_text(error: &DiscoveryError) -> String {
    let key = match error {
        DiscoveryError::AuthRequired => "github-browser-sign-in",
        DiscoveryError::AuthRefused => "github-browser-auth-refused",
        DiscoveryError::Forbidden => "github-browser-forbidden",
        DiscoveryError::NotFound => "github-browser-not-found",
        DiscoveryError::RateLimited { retry_at } => {
            return crate::i18n::message(
                "github-browser-rate-limit",
                &[(
                    "time",
                    retry_at
                        .map(|time| time.to_string())
                        .unwrap_or_else(|| text("github-browser-later").into())
                        .into(),
                )],
            );
        }
        DiscoveryError::Malformed => "github-browser-malformed",
        DiscoveryError::Cancelled => "github-browser-cancelled",
        DiscoveryError::QueueUnavailable => "github-browser-queue",
        DiscoveryError::Transport => "github-browser-transport",
    };
    text(key).into()
}
impl Browser {
    pub(super) fn show(&mut self, ctx: &egui::Context) -> (bool, Option<Intent>) {
        if let Some(root) = self.local.tick(ctx) {
            return (false, Some(Intent::Open(root)));
        }
        let mut intent = None;
        let mut open = None;
        let mut close = false;
        let response =
            egui::Modal::new(egui::Id::new("github-repository-browser")).show(ctx, |ui| {
                ui.set_width(640.0_f32.min(ctx.content_rect().width() - 48.0).max(240.0));
                ui.heading(text("github-browser-title"));
                ui.label(format!(
                    "github.com · {}",
                    if self.session.account.selected() {
                        &self.session.account.user
                    } else {
                        text("github-browser-anonymous")
                    }
                ));
                ui.horizontal_wrapped(|ui| {
                    let previous = self.mode;
                    for (mode, key) in [
                        "github-browser-mine",
                        "github-browser-starred",
                        "github-browser-repositories",
                        "github-browser-owners",
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        ui.selectable_value(&mut self.mode, mode, text(key));
                    }
                    if previous != self.mode {
                        self.reset();
                        self.query = None;
                        self.focus = true;
                    }
                });
                if self.mode >= 2 {
                    let edit = ui.add(
                        egui::TextEdit::singleline(&mut self.input)
                            .char_limit(256)
                            .hint_text(text("github-browser-search-hint"))
                            .desired_width(f32::INFINITY),
                    );
                    if self.focus {
                        edit.request_focus();
                        self.focus = false;
                    }
                    if edit.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                        self.request(self.query_from_controls(), false, ctx);
                    }
                }
                ui.horizontal_wrapped(|ui| {
                    if ui.button(text("github-browser-search")).clicked() {
                        self.request(self.query_from_controls(), false, ctx);
                    }
                    if ui.link(text("github-browser-manual")).clicked() {
                        intent = Some(Intent::Manual);
                    }
                    if ui.link(text("github-browser-account")).clicked() {
                        intent = Some(Intent::Settings);
                    }
                    if ui.button(text("github-browser-close")).clicked() {
                        close = true;
                    }
                });
                if let Some(error) = &self.local.error {
                    ui.label(error);
                }
                if self.mode < 2 && !self.session.account.selected() {
                    ui.label(text("github-browser-sign-in"));
                }
                if let Some(error) = &self.error {
                    ui.label(error_text(error));
                }
                if self.pending.is_some() {
                    ui.add(crate::spinner::Spinner::new());
                    ui.label(text(if self.loaded {
                        "github-browser-loading-more"
                    } else {
                        "github-browser-loading"
                    }));
                    ctx.request_repaint_after(std::time::Duration::from_millis(100));
                }
                if self.incomplete {
                    ui.label(text("github-browser-incomplete"));
                }
                if let Some(total) = self.total {
                    ui.label(crate::i18n::message(
                        "github-browser-total",
                        &[("count", total.into())],
                    ));
                }
                ui.add(
                    egui::TextEdit::singleline(&mut self.filter)
                        .hint_text(text("github-browser-filter"))
                        .desired_width(f32::INFINITY),
                );
                let needle = self.filter.to_lowercase();
                let visible = self
                    .repositories
                    .iter()
                    .enumerate()
                    .filter(|(_, repo)| {
                        format!("{}/{} {}", repo.owner, repo.name, repo.description)
                            .to_lowercase()
                            .contains(&needle)
                    })
                    .map(|(index, _)| index)
                    .collect::<Vec<_>>();
                if self.loaded
                    && self.pending.is_none()
                    && visible.is_empty()
                    && self.owners.is_empty()
                {
                    ui.label(text(if self.repositories.is_empty() {
                        "github-browser-empty"
                    } else {
                        "github-browser-no-matches"
                    }));
                }
                egui::ScrollArea::vertical()
                    .max_height(350.0_f32.min(ctx.content_rect().height() * 0.5))
                    .show_rows(ui, 106.0, visible.len(), |ui, range| {
                        for index in range {
                            let repo = &self.repositories[visible[index]];
                            ui.push_id(repo.id, |ui| {
                                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                                ui.hyperlink_to(
                                    format!("{}/{}", repo.owner, repo.name),
                                    &repo.web_url,
                                );
                                ui.label(repo.description.chars().take(180).collect::<String>());
                                let visibility = text(match repo.visibility {
                                    Visibility::Public => "github-browser-public",
                                    Visibility::Private => "github-browser-private",
                                    Visibility::Internal => "github-browser-internal",
                                });
                                ui.label(format!(
                                    "{visibility} · {} · ★ {}",
                                    repo.language.as_deref().unwrap_or("—"),
                                    repo.stars
                                        .map(|stars| stars.to_string())
                                        .unwrap_or_else(|| "—".into())
                                ));
                                ui.horizontal(|ui| {
                                    if ui.button(text("github-browser-clone")).clicked() {
                                        intent = Some(Intent::Clone {
                                            https: repo.https_url.clone(),
                                            ssh: repo.ssh_url.clone(),
                                        });
                                    }
                                    let paths = self.local.matches(&repo.hosting);
                                    if paths.len() == 1 {
                                        if ui.button(text("github-browser-open-local")).clicked() {
                                            open = Some((paths[0].clone(), repo.hosting.clone()));
                                        }
                                    } else if !paths.is_empty() {
                                        ui.menu_button(text("github-browser-open-local"), |ui| {
                                            for path in paths {
                                                if ui.button(path.display().to_string()).clicked() {
                                                    open = Some((path, repo.hosting.clone()));
                                                    ui.close();
                                                }
                                            }
                                        });
                                    }
                                });
                            });
                        }
                    });
                let mut owner_query = None;
                egui::ScrollArea::vertical()
                    .id_salt("owners")
                    .max_height(200.0)
                    .show(ui, |ui| {
                        for owner in &self.owners {
                            if owner.login.to_lowercase().contains(&needle)
                                && ui
                                    .button(format!(
                                        "{} · {}",
                                        owner.login,
                                        text(if owner.organization {
                                            "github-browser-organization"
                                        } else {
                                            "github-browser-user"
                                        })
                                    ))
                                    .clicked()
                            {
                                owner_query = Some(Query::OwnerRepositories {
                                    login: owner.login.clone(),
                                    organization: owner.organization,
                                });
                            }
                        }
                    });
                if let Some(query) = owner_query {
                    self.request(query, false, ctx);
                }
                if self.next.is_some()
                    && ui
                        .add_enabled(
                            self.pending.is_none(),
                            egui::Button::new(text("github-browser-more")),
                        )
                        .clicked()
                    && let Some(query) = &self.query
                {
                    self.request(query.clone(), true, ctx);
                }
            });
        if let Some((path, hosting)) = open {
            self.local.open(path, hosting, ctx);
        }
        (close || response.should_close(), intent)
    }
}
