//! Views popped out of the main window into windows of their own (the diff, the terminal, a
//! pull request, the CI view): which ones are, the ids and titles of their windows, and what
//! happens to them as views open and close. Drawing them is in [`views`] and [`window`].
//!
//! For the session only: a restart opens every view in the main window again.

mod views;
mod window;

pub(in crate::repo) use views::button;

#[cfg(test)]
mod tests;

use std::path::Path;

/// A view that can live in a window of its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
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
            Panel::Ci => "CI view",
        }
    }
}

/// What the pop-out buttons and the command palette ask for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Request {
    /// Move the view into a window of its own.
    PopOut(Panel),
    /// Put it back into the main window.
    Dock(Panel),
    /// Pop out what the big area shows (the diff, the CI view or the pull request).
    PopShown,
}

/// The views of one repository tab shown in windows of their own.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Popouts {
    popped: [bool; Panel::ALL.len()],
}

impl Popouts {
    fn slot(panel: Panel) -> usize {
        Panel::ALL
            .iter()
            .position(|p| *p == panel)
            .unwrap_or_default()
    }

    /// Whether `panel` is in a window of its own.
    pub(crate) fn is_popped(&self, panel: Panel) -> bool {
        self.popped[Self::slot(panel)]
    }

    /// Moves `panel` into its window; false when it already was there.
    pub(crate) fn pop_out(&mut self, panel: Panel) -> bool {
        !std::mem::replace(&mut self.popped[Self::slot(panel)], true)
    }

    /// Puts `panel` back into the main window (its window was closed, or Bring back was
    /// clicked); false when it was not popped out.
    pub(crate) fn dock(&mut self, panel: Panel) -> bool {
        std::mem::replace(&mut self.popped[Self::slot(panel)], false)
    }

    /// The panels in windows of their own, in [`Panel::ALL`]'s order.
    pub(crate) fn popped(&self) -> impl Iterator<Item = Panel> + '_ {
        Panel::ALL.into_iter().filter(|p| self.is_popped(*p))
    }

    /// Forgets the pop-outs whose view is gone (`open` says which views are open): the diff
    /// closed with Esc, the pull request closed, the terminal hidden. Their windows close, and
    /// the view opens in the main window next time. Returns those forgotten.
    pub(crate) fn forget_closed(&mut self, open: impl Fn(Panel) -> bool) -> Vec<Panel> {
        let gone: Vec<Panel> = self.popped().filter(|p| !open(*p)).collect();
        for panel in &gone {
            self.dock(*panel);
        }
        gone
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

/// The title of a pop-out window: the repository and what it shows, such as
/// "gitcraft — app: diff of src/main.rs". Long names are cut in the middle, where a path's
/// least telling part is.
pub(crate) fn title(repo: &str, what: &str) -> String {
    format!(
        "gitcraft — {}: {}",
        shorten(repo.trim(), MAX_REPO),
        shorten(what.trim(), MAX_WHAT)
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

/// What the command palette offers for popping views out.
pub(crate) fn palette_items() -> Vec<(String, String, super::Action)> {
    use super::Action;
    vec![
        (
            "Pop out the view shown".into(),
            "window".into(),
            Action::Popout(Request::PopShown),
        ),
        (
            "Pop out the terminal".into(),
            "window".into(),
            Action::Popout(Request::PopOut(Panel::Terminal)),
        ),
    ]
}
