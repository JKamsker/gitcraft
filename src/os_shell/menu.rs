//! What the taskbar button's or launcher's menu holds: the recent repositories, then _Clone…_
//! and _Open…_, each starting gitcraft with what it needs; and what gitcraft does when started
//! so.

use std::path::{Path, PathBuf};

/// The argument that starts gitcraft on the clone dialog.
pub const CLONE_ARG: &str = "--clone";
/// The argument that starts gitcraft on the folder picker.
pub const OPEN_ARG: &str = "--open";

/// How many recent repositories the menu lists.
const MAX_RECENT: usize = 8;

/// One item of the menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuItem {
    pub title: String,
    /// What gitcraft is started with.
    pub args: Vec<String>,
    pub tooltip: String,
    /// A recent repository (listed apart from the tasks).
    pub recent: bool,
}

/// The menu for `recent` (newest first): up to [`MAX_RECENT`] of them, named by their folder
/// (and their machine, for one elsewhere), then _Clone…_ and _Open…_.
pub fn menu_items(recent: &[PathBuf]) -> Vec<MenuItem> {
    let mut items: Vec<MenuItem> = Vec::new();
    for path in recent.iter().take(MAX_RECENT) {
        let item = MenuItem {
            title: title(path),
            args: vec![path.to_string_lossy().into_owned()],
            tooltip: crate::remote::display(path),
            recent: true,
        };
        if !items.iter().any(|i| i.args == item.args) {
            items.push(item);
        }
    }
    items.push(MenuItem {
        title: "Clone…".to_owned(),
        args: vec![CLONE_ARG.to_owned()],
        tooltip: "Clone a repository".to_owned(),
        recent: false,
    });
    items.push(MenuItem {
        title: "Open…".to_owned(),
        args: vec![OPEN_ARG.to_owned()],
        tooltip: "Open a repository on this computer".to_owned(),
        recent: false,
    });
    items
}

/// A repository's name in the menu: its folder, and where it is when not here.
fn title(path: &Path) -> String {
    let name = |p: &str| {
        p.trim_end_matches(['/', '\\'])
            .rsplit(['/', '\\'])
            .next()
            .filter(|n| !n.is_empty())
            .unwrap_or(p)
            .to_owned()
    };
    match crate::remote::parse(path) {
        Some(t) => format!("{} ({})", name(&t.path), t.host.label()),
        None => name(&path.to_string_lossy()),
    }
}

/// What gitcraft was started to do, from its first argument.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StartAction {
    Nothing,
    Open(PathBuf),
    /// The clone dialog.
    Clone,
    /// The folder picker.
    PickFolder,
}

pub fn start_action(arg: Option<&std::ffi::OsStr>) -> StartAction {
    match arg {
        None => StartAction::Nothing,
        Some(a) if a == CLONE_ARG => StartAction::Clone,
        Some(a) if a == OPEN_ARG => StartAction::PickFolder,
        Some(a) => StartAction::Open(PathBuf::from(a)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_the_recent_repositories_then_clone_and_open() {
        let recent: Vec<PathBuf> = [
            "/home/me/code/gitcraft",
            "/home/me/my repo é/",
            "ssh://build/home/me/srv",
            "/home/me/code/gitcraft",
        ]
        .iter()
        .map(PathBuf::from)
        .collect();
        let items = menu_items(&recent);
        let titles: Vec<&str> = items.iter().map(|i| i.title.as_str()).collect();
        assert_eq!(
            titles,
            [
                "gitcraft",
                "my repo é",
                "srv (SSH · build)",
                "Clone…",
                "Open…"
            ]
        );
        assert_eq!(items[1].args, ["/home/me/my repo é/"]);
        assert_eq!(items[2].tooltip, "SSH · build  /home/me/srv");
        assert!(items[..3].iter().all(|i| i.recent));
        assert_eq!(items[3].args, [CLONE_ARG]);
        assert_eq!(items[4].args, [OPEN_ARG]);
        assert!(!items[4].recent);
    }

    #[test]
    fn at_most_eight_recent_and_none_is_just_the_tasks() {
        let many: Vec<PathBuf> = (0..20).map(|i| PathBuf::from(format!("/r/{i}"))).collect();
        assert_eq!(menu_items(&many).iter().filter(|i| i.recent).count(), 8);
        let none = menu_items(&[]);
        assert_eq!(none.len(), 2);
    }

    #[test]
    fn the_first_argument_says_what_to_do() {
        use std::ffi::OsStr;
        assert_eq!(start_action(None), StartAction::Nothing);
        assert_eq!(
            start_action(Some(OsStr::new("--clone"))),
            StartAction::Clone
        );
        assert_eq!(
            start_action(Some(OsStr::new("--open"))),
            StartAction::PickFolder
        );
        assert_eq!(
            start_action(Some(OsStr::new("C:\\code\\x y"))),
            StartAction::Open(PathBuf::from("C:\\code\\x y"))
        );
    }
}
