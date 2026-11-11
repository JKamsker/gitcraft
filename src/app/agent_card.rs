//! Agents in the window: their requests handed to the tab of their repository, and the
//! Approve / Deny card for one that can't be undone, shown over whichever tab is active.

use egui::{RichText, Ui};

use super::{App, Tab};
use crate::icons;
use crate::mcp::link::Incoming;
use crate::mcp::wire::{Outcome, Reply};
use crate::repo::agent::CardView;
use crate::theme::{self, Tokens};

/// What the user did with the card.
enum Answer {
    Approve,
    Deny,
}

impl App {
    /// Hands the requests that came in to the tabs of their repositories, or answers them.
    pub(super) fn agent_requests(&mut self, ctx: &egui::Context) {
        for incoming in crate::instance::take_agent_calls() {
            if let Some(window) = self.windows.parked.values_mut().find(|w| {
                w.tabs
                    .iter()
                    .any(|t| matches!(t, Tab::Repo(r) if r.path.to_string_lossy() == incoming.repo))
            }) {
                route(self.settings.agent_writes, &mut window.tabs, incoming, ctx);
            } else {
                route(self.settings.agent_writes, &mut self.tabs, incoming, ctx);
            }
        }
    }

    /// The card of the first tab with a request waiting for the user, over everything.
    pub(super) fn agent_card(&mut self, ctx: &egui::Context) {
        let waiting = self.tabs.iter().enumerate().find_map(|(i, tab)| match tab {
            Tab::Repo(r) => r
                .agent_card()
                .map(|card| (i, r.name.clone(), r.path.display().to_string(), card)),
            _ => None,
        });
        let Some((i, name, path, card)) = waiting else {
            return;
        };
        let t = Tokens::get(ctx);
        let frame = egui::Frame::new()
            .fill(t.card)
            .stroke(egui::Stroke::new(1.0, t.field_border))
            .corner_radius(t.radius_lg)
            .inner_margin(egui::Margin::same(18))
            .shadow(ctx.global_style().visuals.window_shadow);
        let mut answer = None;
        egui::Modal::new(egui::Id::new("gitcraft-agent-card"))
            .frame(frame)
            .backdrop_color(egui::Color32::from_black_alpha(110))
            .show(ctx, |ui| {
                ui.set_width(440.0);
                answer = card_ui(ui, &t, &card, &name, &path);
            });
        let decided = match answer {
            Some(Answer::Approve) => Some(true),
            Some(Answer::Deny) => Some(false),
            None => None,
        };
        if let (Some(approve), Some(Tab::Repo(r))) = (decided, self.tabs.get_mut(i)) {
            r.agent_decide(approve, ctx);
        }
    }
}

/// Hands `incoming` to the tab of its repository, unless agents may not change anything
/// (`writes` off) or no tab has it open; then it is answered here.
fn route(writes: bool, tabs: &mut [Tab], incoming: Incoming, ctx: &egui::Context) {
    // A report changes nothing: it goes to the tab whose background run it is about, which
    // the user started (the agent works in a worktree of its own, no tab's), the worktree it
    // was started from shown or parked behind another.
    if let crate::mcp::wire::WriteOp::Report { run, report } = &incoming.op {
        let tab = tabs.iter_mut().find_map(|tab| match tab {
            Tab::Repo(r) => r.resolver_owner(run),
            _ => None,
        });
        let reply = match tab {
            Some(tab) => {
                tab.resolver_report(report.clone(), ctx);
                Reply::new(Outcome::Done, "noted")
            }
            None => Reply::new(
                Outcome::NotOpen,
                "No gitcraft tab is waiting for this run any more: the user closed it. Stop.",
            ),
        };
        let _ = incoming.reply.send(reply);
        return;
    }
    if !writes {
        let _ = incoming.reply.send(Reply::new(
            Outcome::Disabled,
            "The user turned off changes by agents in gitcraft (Settings → Agents). The reading \
             tools still work.",
        ));
        return;
    }
    let tab = tabs.iter_mut().find_map(|tab| match tab {
        Tab::Repo(r) if r.path.to_string_lossy() == incoming.repo => Some(r),
        _ => None,
    });
    match tab {
        Some(tab) => tab.agent_request(incoming, ctx),
        None => {
            let _ = incoming.reply.send(Reply::new(Outcome::NotOpen, ""));
        }
    }
}

/// `1:05`.
fn countdown(left: std::time::Duration) -> String {
    let s = left.as_secs();
    format!("{}:{:02}", s / 60, s % 60)
}

/// How long Approve takes no click after the card appears: a click then was meant for what
/// the card popped up over.
const ARMING: std::time::Duration = std::time::Duration::from_millis(700);

fn card_ui(ui: &mut Ui, t: &Tokens, card: &CardView, name: &str, path: &str) -> Option<Answer> {
    ui.horizontal(|ui| {
        ui.add(icons::image("bot", 16.0, t.accent));
        // As the agent names itself: plain text, not styled as anything verified.
        ui.label(
            RichText::new(crate::i18n::message(
                "interaction-dialogs-agent",
                &[("agent", card.agent.as_str().into())],
            ))
            .color(t.text)
            .font(theme::medium(13.0)),
        );
    });
    ui.add_space(4.0);
    ui.label(RichText::new(&card.ask.title).font(theme::semibold(15.0)));
    ui.label(
        RichText::new(crate::i18n::message(
            "interaction-dialogs-repo",
            &[("repository", name.into())],
        ))
        .color(t.text_faint)
        .small(),
    )
    .on_hover_text(path);
    ui.add_space(8.0);
    ui.add(egui::Label::new(&card.ask.body).wrap());
    ui.add_space(8.0);
    if let Some(preview) = &card.preview {
        egui::CollapsingHeader::new(crate::i18n::text("interaction-dialogs-ghost"))
            .id_salt("approval-ghost-header")
            .default_open(true)
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("approval-ghost")
                    .max_height(170.0)
                    .show(ui, |ui| {
                        for (name, from, to) in &preview.moves {
                            ui.label(RichText::new(name).strong());
                            ui.horizontal_wrapped(|ui| {
                                ui.colored_label(
                                    t.text_dim,
                                    format!("○ {}", crate::repo::short(from)),
                                );
                                ui.colored_label(t.accent, "→");
                                ui.colored_label(
                                    t.accent.gamma_multiply(0.75),
                                    format!("◌ {}", crate::repo::short(to)),
                                )
                                .on_hover_text(format!("{from} → {to}"));
                            });
                        }
                        for commit in &preview.leaving {
                            ui.colored_label(
                                t.warning,
                                crate::i18n::message(
                                    "interaction-dialogs-leaving",
                                    &[("commit", commit.as_str().into())],
                                ),
                            );
                        }
                        for note in &preview.notes {
                            ui.label(note);
                        }
                    });
            });
    }
    ui.label(
        RichText::new(crate::i18n::text("interaction-dialogs-commands"))
            .color(t.text_dim)
            .small(),
    );
    egui::Frame::new()
        .fill(t.field)
        .corner_radius(t.radius_sm)
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            for command in &card.commands {
                ui.add(egui::Label::new(RichText::new(command).font(theme::mono(11.5))).wrap());
            }
        });
    ui.add_space(8.0);
    let undo_color = if card.undoable { t.text_dim } else { t.warning };
    ui.add(egui::Label::new(RichText::new(&card.ask.undo_note).color(undo_color).small()).wrap());
    ui.label(
        RichText::new(crate::i18n::message(
            "interaction-dialogs-denied",
            &[("remaining", countdown(card.left).into())],
        ))
        .color(t.text_faint)
        .small(),
    );
    ui.add_space(14.0);
    let armed = card.shown_for >= ARMING;
    if !armed {
        ui.ctx().request_repaint_after(ARMING - card.shown_for);
    }
    let mut answer = None;
    ui.horizontal(|ui| {
        // Red only for what can't be undone.
        let fill = if card.undoable { t.accent } else { t.danger };
        let approve = egui::Button::new(
            RichText::new(crate::i18n::text("interaction-dialogs-approve"))
                .color(egui::Color32::WHITE)
                .font(theme::medium(12.5)),
        )
        .fill(fill)
        .min_size(egui::vec2(90.0, 30.0));
        if ui.add_enabled(armed, approve).clicked() {
            answer = Some(Answer::Approve);
        }
        let deny = ui.add(
            egui::Button::new(crate::i18n::text("interaction-dialogs-deny"))
                .min_size(egui::vec2(80.0, 30.0)),
        );
        if deny.clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            answer = Some(Answer::Deny);
        }
    });
    answer
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::wire::WriteOp;
    use crate::repo::RepoTab;

    #[test]
    fn actual_agent_card_translates_decision_and_retains_opaque_request_and_commands() {
        use egui_kittest::{Harness, kittest::Queryable};
        crate::i18n::set_language("de");
        let card = CardView {
            agent: "Agent 日本語".into(),
            preview: None,
            ask: crate::repo::agent::plan::Ask::opaque(
                "user supplied title",
                "user supplied body",
                "opaque undo policy",
            ),
            commands: vec!["git push --force-with-lease origin main".into()],
            undoable: false,
            left: std::time::Duration::from_secs(65),
            shown_for: std::time::Duration::from_secs(1),
        };
        let fonts = std::cell::Cell::new(false);
        let mut h = Harness::new_ui_state(
            move |ui, decided: &mut Option<bool>| {
                if !fonts.replace(true) {
                    crate::theme::install_fonts(ui.ctx());
                    return;
                }
                let t = Tokens::get(ui.ctx());
                if let Some(answer) = card_ui(ui, &t, &card, "repo 日本語", "/opaque/path") {
                    *decided = Some(matches!(answer, Answer::Approve));
                }
            },
            None,
        );
        h.run();
        h.get_by_label("Agent 日本語 möchte Folgendes tun:");
        h.get_by_label("git push --force-with-lease origin main");
        h.get_by_label(
            "Ohne Antwort wird die Anfrage in 1:05 automatisch abgelehnt. Esc lehnt ab.",
        );
        h.get_by_label("Ablehnen").click();
        h.run();
        assert_eq!(*h.state(), Some(false));
        h.get_by_label("Genehmigen").click();
        h.run();
        assert_eq!(*h.state(), Some(true));
        crate::i18n::set_language("en-US");
    }

    /// What the window answers at once for a request about `repo`.
    fn routed(writes: bool, tabs: &mut [Tab], repo: &str) -> Option<Reply> {
        let ctx = egui::Context::default();
        let (reply, rx) = std::sync::mpsc::channel();
        let incoming = Incoming {
            repo: repo.to_owned(),
            agent: "a".into(),
            op: WriteOp::Push { force: true },
            reply,
            gone: Default::default(),
        };
        route(writes, tabs, incoming, &ctx);
        rx.try_recv().ok()
    }

    #[test]
    fn with_changes_turned_off_nothing_reaches_a_tab() {
        let ctx = egui::Context::default();
        let path = std::env::temp_dir().join("gitcraft-route-off");
        let repo = path.to_string_lossy().into_owned();
        let mut tabs = [Tab::Repo(Box::new(RepoTab::new_deferred(path, &ctx)))];
        let reply = routed(false, &mut tabs, &repo);
        assert_eq!(reply.as_ref().map(|r| r.outcome), Some(Outcome::Disabled));
        assert!(reply.is_some_and(|r| r.message.contains("Settings → Agents")));
    }

    #[test]
    fn a_repository_no_tab_has_open_is_passed_on() {
        let mut tabs = [Tab::Home, Tab::Settings];
        let reply = routed(true, &mut tabs, "/somewhere");
        assert_eq!(reply.map(|r| r.outcome), Some(Outcome::NotOpen));
    }

    #[test]
    fn a_request_goes_to_the_tab_of_its_repository() {
        let ctx = egui::Context::default();
        let path = std::env::temp_dir().join("gitcraft-route-on");
        let repo = path.to_string_lossy().into_owned();
        let mut tabs = [Tab::Repo(Box::new(RepoTab::new_deferred(path, &ctx)))];
        // Taken by the tab, which answers once it has planned it (not here, at once).
        assert!(routed(true, &mut tabs, &repo).is_none());
    }

    #[test]
    fn the_wait_reads_as_minutes_and_seconds() {
        assert_eq!(countdown(std::time::Duration::from_secs(120)), "2:00");
        assert_eq!(countdown(std::time::Duration::from_secs(65)), "1:05");
        assert_eq!(countdown(std::time::Duration::from_millis(900)), "0:00");
    }
}
