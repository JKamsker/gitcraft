//! The order the graph reads history in (topological, commit date, author date) and _First
//! parent only_: chosen from the graph's header, kept per repository with the app.

use std::path::Path;

use egui::{RichText, Ui};

use super::{Action, RepoTab};
use crate::git::{LogOrder, Order};
use crate::theme::Tokens;

fn id(repo: &Path) -> egui::Id {
    egui::Id::new(("graph-order", repo))
}

/// What is kept for the repository `repo` (its main worktree's folder).
pub(super) fn load(ctx: &egui::Context, repo: &Path) -> LogOrder {
    ctx.data_mut(|d| d.get_persisted::<LogOrder>(id(repo)))
        .unwrap_or_default()
}

fn store(ctx: &egui::Context, repo: &Path, order: LogOrder) {
    ctx.data_mut(|d| {
        if order.is_default() {
            d.remove::<LogOrder>(id(repo));
        } else {
            d.insert_persisted(id(repo), order);
        }
    });
}

/// The header's label of the graph column: `GRAPH`, and how it is read when not by default.
pub(super) fn header_label(order: LogOrder) -> String {
    match order.summary() {
        Some(summary) => crate::i18n::message(
            "repo-core-2585",
            &[("to-uppercase", summary.to_uppercase().to_string().into())],
        ),
        None => crate::i18n::text("repo-core-2586").to_owned(),
    }
}

/// The menu's items: one order of the three, and first parent only. Asks for the reading
/// chosen when it differs from `current`.
pub(super) fn menu_items(ui: &mut Ui, current: LogOrder, acts: &mut Vec<Action>) {
    let t = Tokens::get(ui.ctx());
    let mut next = current;
    ui.label(
        RichText::new(crate::i18n::text("repo-core-2587"))
            .color(t.text_faint)
            .small(),
    );
    for order in Order::ALL {
        if ui
            .radio(next.order == order, order.label())
            .on_hover_text(order.tip())
            .clicked()
        {
            next.order = order;
        }
    }
    ui.separator();
    ui.checkbox(&mut next.first_parent, crate::i18n::text("repo-core-2588"))
        .on_hover_text(crate::i18n::text("repo-core-2589"));
    if next != current {
        acts.push(Action::GraphOrder(next));
    }
}

impl RepoTab {
    /// Reads history in `order` from now on, for this repository, and reads it again.
    pub(super) fn set_log_order(&mut self, order: LogOrder, ctx: &egui::Context) {
        store(ctx, self.repo_key(), order);
        if order != self.log_order {
            self.search.retire();
            self.reload(ctx);
        }
    }

    /// Takes the order the history just read was read in; reads again if the one kept for the
    /// repository differs (changed while it was read, or read before the repository was known:
    /// a worktree's is its main worktree's).
    pub(super) fn take_order(&mut self, read_in: LogOrder, ctx: &egui::Context) {
        if self.log_order != read_in {
            self.search.retire();
        }
        if load(ctx, self.repo_key()) != read_in {
            self.reload_again = true;
        }
        self.log_order = read_in;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_order_is_kept_per_repository_and_the_default_is_not_stored() {
        let ctx = egui::Context::default();
        let (a, b) = (Path::new("/repos/a b ✓"), Path::new("/repos/other"));
        let first = LogOrder {
            order: Order::Topo,
            first_parent: true,
        };
        store(&ctx, a, first);
        assert_eq!(load(&ctx, a), first);
        assert_eq!(load(&ctx, b), LogOrder::default());
        store(&ctx, a, LogOrder::default());
        assert!(
            ctx.data_mut(|d| d.get_persisted::<LogOrder>(id(a)))
                .is_none()
        );
    }

    #[test]
    fn the_header_says_how_history_is_read() {
        assert_eq!(header_label(LogOrder::default()), "GRAPH");
        let author = LogOrder {
            order: Order::AuthorDate,
            first_parent: false,
        };
        assert_eq!(header_label(author), "GRAPH · AUTHOR DATE");
    }

    /// Through the tab: the order kept for the repository is the one its history is read in,
    /// and choosing another from the menu reads it again so.
    #[test]
    fn the_tab_reads_history_in_the_order_kept_and_again_when_changed() {
        let dir = std::env::temp_dir().join(format!("gitcraft-graph order-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        let git = |args: &[&str]| crate::git::run(Some(&dir), args);
        for args in [
            &["init", "-q", "-b", "main"][..],
            &["config", "user.name", "t"],
            &["config", "user.email", "t@t"],
            &["commit", "-q", "--allow-empty", "-m", "base"],
            &["switch", "-q", "-c", "pr/ünï"],
            &["commit", "-q", "--allow-empty", "-m", "side one"],
            &["commit", "-q", "--allow-empty", "-m", "side two"],
            &["switch", "-q", "main"],
            &[
                "merge",
                "-q",
                "--no-ff",
                "-m",
                "Merge pull request",
                "pr/ünï",
            ],
            &["branch", "-q", "-D", "pr/ünï"],
        ] {
            assert!(git(args).is_ok(), "{args:?}");
        }
        let ctx = egui::Context::default();
        let first = LogOrder {
            order: Order::Topo,
            first_parent: true,
        };
        let settle = |tab: &mut RepoTab| {
            for _ in 0..400 {
                tab.poll(&ctx);
                if !tab.loading && !tab.history.is_empty() {
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
        };
        let mut tab = RepoTab::new(dir.clone(), &ctx);
        settle(&mut tab);
        let at_first = tab.history.len();
        tab.apply(Action::GraphOrder(first), &ctx);
        settle(&mut tab);
        let (shown, width, order) = (tab.history.len(), tab.graph.width, tab.log_order);
        // Kept: another tab on the repository reads it so from the start.
        let mut other = RepoTab::new(dir.clone(), &ctx);
        settle(&mut other);
        let other_shown = other.history.len();
        tab.apply(Action::GraphOrder(LogOrder::default()), &ctx);
        settle(&mut tab);
        let (all, all_width) = (tab.history.len(), tab.graph.width);
        let kept = load(&ctx, tab.repo_key());
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(at_first, 4);
        assert_eq!((shown, width, order), (2, 1, first));
        assert_eq!(other_shown, 2);
        assert_eq!((all, all_width), (4, 2));
        assert_eq!(kept, LogOrder::default());
    }
}
