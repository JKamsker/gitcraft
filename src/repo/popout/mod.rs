//! Views popped out of the main window into windows of their own (the diff, the terminal, a
//! pull request, the CI view): which ones are, the ids and titles of their windows, and what
//! happens to them as views open and close. Drawing them is in [`views`] and [`window`].
//!
//! A pop-out lasts until it is closed on purpose: its window's close button, Bring back, or
//! closing the view itself (Esc, its ×). A view that goes away for another reason (selecting a
//! commit closes the diff) leaves its window open, saying so, and the next one of its kind
//! opens there. For the session only: a restart opens every view in the main window again.

mod hint;
mod views;
mod window;

#[cfg(test)]
mod tests;

use std::path::Path;

use super::Action;

pub(in crate::repo) use views::{button, close_tip};

/// A view that can live in a window of its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Panel {
    Diff,
    Terminal,
    Pr,
    Ci,
}

impl Panel {
    /// Every one, in the order their windows are drawn.
    pub(crate) const ALL: [Panel; 4] = [Panel::Diff, Panel::Terminal, Panel::Pr, Panel::Ci];

    /// What it is called in tooltips and messages.
    pub(crate) fn noun(self) -> &'static str {
        match self {
            Panel::Diff => "diff",
            Panel::Terminal => "terminal",
            Panel::Pr => "pull request",
            Panel::Ci => crate::i18n::text("repo-core-4206"),
        }
    }

    fn slot(self) -> usize {
        match self {
            Panel::Diff => 0,
            Panel::Terminal => 1,
            Panel::Pr => 2,
            Panel::Ci => 3,
        }
    }
}

/// What the pop-out buttons, the main window's hint and the command palette ask for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Request {
    /// Move the view into a window of its own.
    PopOut(Panel),
    /// Put it back into the main window, the view still open.
    Dock(Panel),
    /// Bring every view of the tab back into the main window.
    DockAll,
    /// Close the view and its window (the terminal: hide it, its shells keep running).
    Close(Panel),
    /// Raise the view's window above the others.
    Focus(Panel),
    /// Pop out what the big area shows (the diff, the CI view or the pull request).
    PopShown,
}

/// The views of one repository tab shown in windows of their own.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Popouts {
    popped: [bool; Panel::ALL.len()],
    /// Popped out this frame, after the main window drew it: its window opens on the next one,
    /// so the view isn't drawn twice in one frame.
    opening: [bool; Panel::ALL.len()],
}

impl Popouts {
    /// Whether `panel` is in a window of its own.
    pub(crate) fn is_popped(&self, panel: Panel) -> bool {
        self.popped[panel.slot()]
    }

    /// Moves `panel` into its window; false when it already was there.
    pub(crate) fn pop_out(&mut self, panel: Panel) -> bool {
        let fresh = !std::mem::replace(&mut self.popped[panel.slot()], true);
        if fresh {
            self.opening[panel.slot()] = true;
        }
        fresh
    }

    /// Puts `panel` back into the main window; false when it was not popped out.
    pub(crate) fn dock(&mut self, panel: Panel) -> bool {
        self.opening[panel.slot()] = false;
        std::mem::replace(&mut self.popped[panel.slot()], false)
    }

    /// Brings every view back into the main window; false when none was out.
    pub(crate) fn dock_all(&mut self) -> bool {
        let any = self.any();
        *self = Self::default();
        any
    }

    /// Whether any view is in a window of its own.
    pub(crate) fn any(&self) -> bool {
        self.popped.contains(&true)
    }

    /// The panels in windows of their own, in [`Panel::ALL`]'s order.
    pub(crate) fn popped(&self) -> impl Iterator<Item = Panel> + '_ {
        Panel::ALL.into_iter().filter(|p| self.is_popped(*p))
    }

    /// The panels whose windows are drawn this frame: those popped out, but not the ones popped
    /// out during it (they open on the next). Takes the frame's newly popped as seen.
    pub(crate) fn take_drawn(&mut self) -> Vec<Panel> {
        let popped: Vec<Panel> = self.popped().collect();
        let draw = popped
            .into_iter()
            .filter(|p| !self.opening[p.slot()])
            .collect();
        self.opening = [false; Panel::ALL.len()];
        draw
    }

    /// Takes in `action` from either window: a view closed on purpose takes its pop-out with
    /// it (`terminals`: how many shells the tab has, for closing the last one).
    pub(crate) fn note(&mut self, action: &Action, terminals: usize) {
        if let Some(panel) = closes(action, terminals) {
            self.dock(panel);
        }
    }
}

/// The view `action` closes on purpose, if any (`terminals`: how many shells there are).
pub(crate) fn closes(action: &Action, terminals: usize) -> Option<Panel> {
    match action {
        Action::CloseDiff => Some(Panel::Diff),
        Action::ClosePr => Some(Panel::Pr),
        Action::CloseCi => Some(Panel::Ci),
        Action::CloseTerminal(_) if terminals <= 1 => Some(Panel::Terminal),
        Action::Popout(Request::Close(panel)) => Some(*panel),
        _ => None,
    }
}

/// The window of `panel` for the repository tab of `tab`: the same one each time it pops out,
/// and another one for every tab, so two tabs' diffs don't share a window.
pub(crate) fn viewport_id(tab: &Path, panel: Panel) -> egui::ViewportId {
    egui::ViewportId::from_hash_of(("gitcraft-popout", tab, panel))
}

/// Longest repository name in a window's title, in characters.
const MAX_REPO: usize = 40;
/// Longest description of what a window shows in its title, in characters.
const MAX_WHAT: usize = 90;

/// The title of a pop-out window: what it shows, the repository, and the app, such as
/// "Diff of src/main.rs — app — GitCraft" (the main window is "GitCraft"). Long names are cut
/// in the middle, where a path's least telling part is.
pub(crate) fn title(repo: &str, what: &str) -> String {
    crate::i18n::message(
        "repo-core-4208",
        &[
            (
                "MAX-WHAT",
                format!("{}", shorten(what.trim(), MAX_WHAT)).into(),
            ),
            (
                "MAX-REPO",
                format!("{}", shorten(repo.trim(), MAX_REPO)).into(),
            ),
        ],
    )
}

/// `text` cut to at most `max` characters, with "…" in place of its middle.
pub(crate) fn shorten(text: &str, max: usize) -> String {
    let count = text.chars().count();
    if count <= max {
        return text.to_owned();
    }
    let keep = max.saturating_sub(1);
    let head = keep / 2;
    let tail = keep - head;
    let start: String = text.chars().take(head).collect();
    let end: String = text.chars().skip(count - tail).collect();
    format!("{start}…{end}")
}

/// What the command palette offers for popping views out and bringing them back.
pub(crate) fn palette_items() -> Vec<(String, String, Action)> {
    vec![
        (
            crate::i18n::text("repo-core-4210").into(),
            "window".into(),
            Action::Popout(Request::PopShown),
        ),
        (
            crate::i18n::text("repo-core-4212").into(),
            "window".into(),
            Action::Popout(Request::PopOut(Panel::Terminal)),
        ),
        (
            crate::i18n::text("repo-core-4213").into(),
            "window".into(),
            Action::Popout(Request::DockAll),
        ),
    ]
}
