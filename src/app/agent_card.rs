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
        for incoming in self.agent_inbox.take() {
            route(self.settings.agent_writes, &mut self.tabs, incoming, ctx);
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

fn card_ui(ui: &mut Ui, t: &Tokens, card: &CardView, name: &str, path: &str) -> Option<Answer> {
    ui.horizontal(|ui| {
        ui.add(icons::image("bot", 16.0, t.accent));
        ui.label(
            RichText::new(format!("{} asks", card.agent))
                .color(t.text_dim)
                .font(theme::medium(12.5)),
        );
    });
    ui.add_space(4.0);
    ui.label(RichText::new(&card.ask.title).font(theme::semibold(15.0)));
    ui.label(
        RichText::new(format!("In {name} · {path}"))
            .color(t.text_faint)
            .small(),
    );
    ui.add_space(8.0);
    ui.add(egui::Label::new(&card.ask.body).wrap());
    ui.add_space(8.0);
    ui.label(RichText::new("It runs:").color(t.text_dim).small());
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
    let (undo_text, undo_color) = if card.undoable {
        (
            "Once done, Ctrl+Z undoes it, as any step of gitcraft's.",
            t.text_dim,
        )
    } else {
        ("This can't be undone.", t.warning)
    };
    ui.label(RichText::new(undo_text).color(undo_color).small());
    ui.label(
        RichText::new(format!(
            "Denied by itself in {} if you don't answer.",
            countdown(card.left)
        ))
        .color(t.text_faint)
        .small(),
    );
    ui.add_space(14.0);
    let mut answer = None;
    ui.horizontal(|ui| {
        let approve = egui::Button::new(
            RichText::new("Approve")
                .color(egui::Color32::WHITE)
                .font(theme::medium(12.5)),
        )
        .fill(t.danger)
        .min_size(egui::vec2(90.0, 30.0));
        if ui.add(approve).clicked() {
            answer = Some(Answer::Approve);
        }
        let deny = ui.add(egui::Button::new("Deny").min_size(egui::vec2(80.0, 30.0)));
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

    /// What the window answers at once for a request about `repo`.
    fn routed(writes: bool, tabs: &mut [Tab], repo: &str) -> Option<Reply> {
        let ctx = egui::Context::default();
        let (reply, rx) = std::sync::mpsc::channel();
        let incoming = Incoming {
            repo: repo.to_owned(),
            agent: "a".into(),
            op: WriteOp::Push { force: true },
            reply,
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
