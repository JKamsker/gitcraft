//! What a tab shows of offline and metered mode (see [`crate::network`]): a chip in the
//! toolbar saying that gitcraft stays off the network and when it last fetched, the network
//! buttons off with why, and the switch in the palette.

use egui::Ui;

use super::{Action, RepoTab, widgets};
use crate::network::{self, Pause};
use crate::settings::Settings;
use crate::theme::Tokens;

/// Why the network buttons are off, when offline mode is on.
pub(super) fn refused_text() -> Option<String> {
    network::refused().map(|p| format!("{}: turn it off to fetch, pull or push", p.why()))
}

/// The chip's text: "Offline · last fetched 3 h ago".
pub(super) fn chip_text(pause: Pause, since_fetch: std::time::Duration) -> String {
    format!("{} · last fetched {} ago", pause.label(), ago(since_fetch))
}

/// What is paused, and how to get it going again, for the chip's hover.
pub(super) fn chip_tip(pause: Pause) -> String {
    let what = "The background fetch, avatars, update checks and downloads and the polling of \
                builds and pull requests are paused.";
    match pause {
        Pause::Switch => format!(
            "{}. {what} Fetch, Pull, Push and the like don't run.\nClick to go online.",
            pause.why()
        ),
        Pause::NoConnection => format!(
            "{}. {what} What you ask for still runs.\nThey start again by themselves once \
             there is a connection.",
            pause.why()
        ),
        Pause::Metered => format!(
            "{}. {what} What you ask for still runs.\nSettings → Git can let them run on a \
             metered connection too.",
            pause.why()
        ),
    }
}

/// "12 s", "3 min", "2 h", "4 days".
fn ago(d: std::time::Duration) -> String {
    match d.as_secs() {
        s @ 0..60 => format!("{s} s"),
        s @ 60..3600 => format!("{} min", s / 60),
        s @ 3600..172_800 => format!("{} h", s / 3600),
        s => format!("{} days", s / 86_400),
    }
}

/// Turns offline mode on or off.
pub(super) fn set_offline(ctx: &egui::Context, offline: bool) {
    let mut settings = Settings::get(ctx);
    if settings.offline != offline {
        settings.offline = offline;
        settings.set(ctx);
        network::set_choices(settings.offline, settings.pause_on_metered);
    }
}

impl RepoTab {
    /// The chip saying gitcraft stays off the network, when it does; a click goes online
    /// when that is offline mode.
    pub(super) fn network_chip(&self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let Some(pause) = network::background_paused() else {
            return;
        };
        let t = Tokens::get(ui.ctx());
        ui.add_space(10.0);
        let color = match pause {
            Pause::Switch | Pause::NoConnection => t.warning,
            Pause::Metered => t.text_dim,
        };
        let chip = widgets::chip(ui, &chip_text(pause, self.last_fetch.elapsed()), color)
            .on_hover_text(chip_tip(pause));
        if pause == Pause::Switch && chip.interact(egui::Sense::click()).clicked() {
            acts.push(Action::SetOffline(false));
        }
        // The time since the last fetch moves on.
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_secs(30));
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    use super::*;
    use crate::git;
    use crate::network::test_pause;
    use crate::repo::{OpKind, Refresh, ops};

    fn repo(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("gitcraft-offline {name} {}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        for args in [
            &["init", "-q"][..],
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                "b",
            ],
        ] {
            assert!(git::run(Some(&dir), args).is_ok());
        }
        dir
    }

    /// In offline mode, what goes on the network is not started and says why; the rest runs.
    #[test]
    fn offline_mode_refuses_network_operations_with_why() {
        let dir = repo("refuse");
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
        let _offline = test_pause::set(Some(Pause::Switch));
        tab.run_ops(
            "Fetch",
            ops(&[&["fetch", "--all"]]),
            Refresh::Status,
            OpKind::plain(),
            &ctx,
        );
        let fetch = (tab.busy_op.clone(), tab.status.clone());
        tab.run_ops(
            "Tag",
            ops(&[&["tag", "t"]]),
            Refresh::Status,
            OpKind::plain(),
            &ctx,
        );
        let tag = tab.busy_op.clone();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(fetch.0, None);
        let (msg, err) = fetch.1.unwrap_or_default();
        assert!(err);
        assert!(
            msg.starts_with("Fetch: not started. Offline mode is on"),
            "{msg}"
        );
        assert_eq!(tag.as_deref(), Some("Tag"));
        assert!(refused_text().is_some_and(|t| t.contains("Offline mode is on")));
    }

    /// Detected without a connection or metered, only the background work pauses: what the
    /// user asks for still runs.
    #[test]
    fn no_connection_and_metered_pause_only_the_background() {
        for pause in [Pause::NoConnection, Pause::Metered] {
            let _paused = test_pause::set(Some(pause));
            assert_eq!(network::background_paused(), Some(pause));
            assert_eq!(network::refused(), None);
            assert_eq!(refused_text(), None);
            assert_eq!(
                super::super::operations::offline_refusal(&ops(&[&["push"]])),
                None
            );
        }
        let _offline = test_pause::set(Some(Pause::Switch));
        assert!(super::super::operations::offline_refusal(&ops(&[&["push"]])).is_some());
        assert_eq!(
            super::super::operations::offline_refusal(&ops(&[&["status"]])),
            None
        );
    }

    /// The background fetch doesn't start while paused, and does once it isn't.
    #[test]
    fn the_background_fetch_waits_while_paused() {
        let dir = repo("background");
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
        tab.deferred = false;
        tab.remotes = vec![git::Remote {
            name: "origin".to_owned(),
            url: "/nowhere".to_owned(),
        }];
        let due = || Instant::now().checked_sub(Duration::from_secs(24 * 3600));
        let mut started = Vec::new();
        for pause in [
            Some(Pause::Metered),
            Some(Pause::NoConnection),
            Some(Pause::Switch),
            None,
        ] {
            let _paused = test_pause::set(pause);
            tab.busy_op = None;
            if let Some(due) = due() {
                tab.last_fetch = due;
            }
            tab.poll(&ctx);
            started.push(tab.busy_op.clone());
        }
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(
            started,
            [None, None, None, Some("Background fetch".to_owned())]
        );
    }

    #[test]
    fn the_chip_says_why_and_when_it_last_fetched() {
        assert_eq!(
            chip_text(Pause::Switch, Duration::from_secs(3 * 3600 + 5)),
            "Offline · last fetched 3 h ago"
        );
        assert_eq!(
            chip_text(Pause::Metered, Duration::from_secs(90)),
            "Metered network · last fetched 1 min ago"
        );
        assert_eq!(
            chip_text(Pause::NoConnection, Duration::from_secs(3 * 86_400)),
            "No connection · last fetched 3 days ago"
        );
        assert!(chip_tip(Pause::Switch).contains("Click to go online"));
        assert!(chip_tip(Pause::Metered).contains("still runs"));
    }
}
