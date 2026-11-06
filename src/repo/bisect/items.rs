//! Where bisecting is offered outside its banner: a commit's menu, the command palette, and the
//! colours its marks show in on the graph.

use egui::{Color32, Ui};

use super::BisectAct;
use crate::git::Mark;
use crate::repo::{Action, Dialog, RepoTab, short};
use crate::theme::Tokens;

/// The colour a commit marked so shows in: the theme's own for success, danger and faint text.
pub(in crate::repo) fn mark_color(t: &Tokens, mark: Mark) -> Color32 {
    match mark {
        Mark::Good => t.success,
        Mark::Bad => t.danger,
        Mark::Skip => t.text_faint,
    }
}

impl RepoTab {
    /// A commit's bisect entries: start one there, or mark it in the one going on.
    pub(in crate::repo) fn bisect_menu_items(
        &self,
        ui: &mut Ui,
        hash: &str,
        idle: bool,
        acts: &mut Vec<Action>,
    ) {
        let Some(bisect) = self.bisect.state() else {
            let blocker = self.bisect_blocker();
            let resp = ui
                .add_enabled(blocker.is_none(), egui::Button::new("Start bisect here…"))
                .on_hover_text("Find the commit that brought a change in, by testing commits between a good and a bad one");
            let resp = match &blocker {
                Some(why) => resp.on_disabled_hover_text(why.as_str()),
                None => resp,
            };
            if resp.clicked() {
                acts.push(Action::Dialog(self.start_bisect_dialog(hash)));
            }
            return;
        };
        let at = short(hash);
        let running = self.bisect.running();
        for (mark, label) in [
            (
                Mark::Good,
                format!("Mark {at} as {}", bisect.word(Mark::Good)),
            ),
            (
                Mark::Bad,
                format!("Mark {at} as {}", bisect.word(Mark::Bad)),
            ),
            (Mark::Skip, format!("Skip {at} in the bisect")),
        ] {
            if ui
                .add_enabled(idle && !running, egui::Button::new(label))
                .clicked()
            {
                acts.push(Action::Bisect(BisectAct::Mark {
                    mark,
                    rev: Some(hash.to_owned()),
                }));
            }
        }
    }

    /// The dialog that starts a bisect from the commit `hash`: it is good and HEAD bad, unless
    /// it is HEAD, which is then the bad one.
    pub(in crate::repo) fn start_bisect_dialog(&self, hash: &str) -> Dialog {
        Dialog::start_bisect(hash, hash != self.refs.head)
    }

    /// What the command palette offers for bisecting.
    pub(in crate::repo) fn bisect_palette_items(&self) -> Vec<(String, String, Action)> {
        let item = |label: String, action| (label, "bisect".to_owned(), action);
        let Some(bisect) = self.bisect.state() else {
            if self.refs.head.is_empty() {
                return Vec::new();
            }
            return vec![item(
                "Start bisect…".to_owned(),
                Action::Dialog(Dialog::start_bisect(&self.refs.head, false)),
            )];
        };
        let mark = |mark| Action::Bisect(BisectAct::Mark { mark, rev: None });
        let mut items = Vec::new();
        if bisect.outcome.is_none() {
            items.extend([
                item(
                    format!("Bisect: mark as {}", bisect.word(Mark::Good)),
                    mark(Mark::Good),
                ),
                item(
                    format!("Bisect: mark as {}", bisect.word(Mark::Bad)),
                    mark(Mark::Bad),
                ),
                item("Bisect: skip this commit".to_owned(), mark(Mark::Skip)),
                item(
                    "Bisect: run a command…".to_owned(),
                    self.bisect_run_dialog(),
                ),
            ]);
        }
        items.push(item("Bisect: stop".to_owned(), self.stop_bisect_action()));
        items
    }
}
