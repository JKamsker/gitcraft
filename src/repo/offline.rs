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

/// The chip's text: "Offline · last fetched 3 h ago" (or "never fetched").
pub(super) fn chip_text(pause: Pause, since_fetch: Option<std::time::Duration>) -> String {
    match since_fetch {
        Some(d) => format!("{} · last fetched {} ago", pause.label(), ago(d)),
        None => format!("{} · never fetched", pause.label()),
    }
}

/// When `FETCH_HEAD` of the repository at `path` (one of this machine) was written.
fn last_fetched(path: &std::path::Path) -> Option<std::time::SystemTime> {
    if crate::remote::is_remote(path) {
        return None;
    }
    let file = crate::git::run(
        Some(path),
        &[
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            "FETCH_HEAD",
        ],
    )
    .ok()?;
    std::fs::metadata(file.trim())
        .and_then(|m| m.modified())
        .ok()
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

/// "under a minute", "3 min", "2 h", "4 days" (the chip is repainted every half minute).
fn ago(d: std::time::Duration) -> String {
    match d.as_secs() {
        0..60 => "under a minute".to_owned(),
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

/// Whether the background fetch of the repository at `path` may run: as other background
/// work, except that one over SSH fetches on that machine's network, which a metered
/// connection here doesn't pay for.
pub(super) fn background_fetch_may_run(path: &std::path::Path) -> bool {
    if network::background_may_run() {
        return true;
    }
    let over_ssh =
        crate::remote::parse(path).is_some_and(|t| matches!(t.host, crate::remote::Host::Ssh(_)));
    over_ssh && network::background_paused() == Some(Pause::Metered)
}

impl RepoTab {
    /// Once background work may go on the network again (the connection came back, offline
    /// mode was turned off, the system answered first): the forge is asked what it was not
    /// asked meanwhile (pull requests, builds, avatars), and when this repository was last
    /// fetched is read.
    pub(super) fn follow_network(&mut self, ctx: &egui::Context) {
        let may_run = network::background_may_run();
        let was = std::mem::replace(&mut self.network_paused, !may_run);
        if was && may_run && self.history_gen > 0 {
            self.avatars_hinted = false;
            self.prs_loaded = None;
            self.load_prs(ctx);
            self.refresh_ci(ctx);
            self.hint_avatars(ctx);
        }
        if !self.fetch_time_asked {
            self.fetch_time_asked = true;
            let (path, slot) = (self.path.clone(), std::sync::Arc::clone(&self.fetched_at));
            crate::connections::spawn(move || {
                let at = last_fetched(&path);
                if let Ok(mut s) = slot.lock() {
                    *s = s.or(at);
                }
            });
        }
    }

    /// When this repository was last fetched: by gitcraft since it started, or else as
    /// `FETCH_HEAD` says (another machine's isn't read: its clock isn't this one's).
    fn fetched_ago(&self) -> Option<std::time::Duration> {
        let at = self.fetched_at.lock().ok().and_then(|a| *a)?;
        std::time::SystemTime::now().duration_since(at).ok()
    }

    /// Notes a fetch that went through, for the chip.
    pub(super) fn note_fetched(&self) {
        if let Ok(mut at) = self.fetched_at.lock() {
            *at = Some(std::time::SystemTime::now());
        }
    }

    /// The chip saying gitcraft stays off the network, when it does; a click goes online
    /// when that is offline mode.
    pub(super) fn network_chip(&self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let Some(pause) = network::background_paused() else {
            return;
        };
        let t = Tokens::get(ui.ctx());
        ui.add_space(10.0);
        let color = t.warning;
        let chip = widgets::chip(ui, &chip_text(pause, self.fetched_ago()), color)
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
        tab.life.shown(Instant::now());
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
            chip_text(Pause::Switch, Some(Duration::from_secs(3 * 3600 + 5))),
            "Offline · last fetched 3 h ago"
        );
        assert_eq!(
            chip_text(Pause::Metered, Some(Duration::from_secs(90))),
            "Metered network · last fetched 1 min ago"
        );
        assert_eq!(
            chip_text(Pause::NoConnection, Some(Duration::from_secs(3 * 86_400))),
            "No connection · last fetched 3 days ago"
        );
        assert_eq!(
            chip_text(Pause::Switch, Some(Duration::from_secs(5))),
            "Offline · last fetched under a minute ago"
        );
        assert_eq!(chip_text(Pause::Switch, None), "Offline · never fetched");
        assert!(chip_tip(Pause::Switch).contains("Click to go online"));
        assert!(chip_tip(Pause::Metered).contains("still runs"));
    }

    /// Only what reaches a remote is refused offline: not the `remote` commands that only
    /// write the config, nor `submodule update` (after a checkout).
    #[test]
    fn only_what_reaches_a_remote_is_refused_offline() {
        use super::super::operations::reaches_a_remote;
        let op = |a: &[&str]| a.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        for (args, reaches) in [
            (&["fetch", "--all"][..], true),
            (&["-c", "push.default=simple", "push", "-u", "o"], true),
            (&["pull", "--rebase"], true),
            (&["ls-remote", "o"], true),
            (&["remote", "update"], true),
            (&["remote", "prune", "origin"], true),
            (&["remote", "show", "origin"], true),
            (&["remote", "show", "-n", "origin"], false),
            (&["remote", "add", "o", "https://x/y"], false),
            (&["remote", "set-url", "o", "https://x/y"], false),
            (&["remote", "rename", "o", "p"], false),
            (&["remote", "remove", "o"], false),
            (&["submodule", "update", "--init", "--recursive"], false),
            (&["lfs", "pull"], true),
            (&["lfs", "ls-files"], false),
            (&["status"], false),
            (&[], false),
        ] {
            assert_eq!(reaches_a_remote(&op(args)), reaches, "{args:?}");
        }
    }

    /// A repository over SSH fetches on that machine's network: a metered connection here
    /// doesn't stop its background fetch; offline mode or no connection does.
    #[test]
    fn an_ssh_repository_still_fetches_on_a_metered_connection_here() {
        let ssh = std::path::Path::new("ssh://build/home/me/r");
        let here = std::path::Path::new("/home/me/r");
        let _metered = test_pause::set(Some(Pause::Metered));
        assert!(background_fetch_may_run(ssh));
        assert!(!background_fetch_may_run(here));
        drop(_metered);
        for pause in [Pause::Switch, Pause::NoConnection] {
            let _p = test_pause::set(Some(pause));
            assert!(!background_fetch_may_run(ssh));
        }
    }

    /// Back on the network, the forge is asked again what it wasn't asked meanwhile.
    #[test]
    fn coming_back_asks_the_forge_again() {
        let dir = repo("resume");
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
        tab.history_gen = 1;
        tab.avatars_hinted = true;
        let asked = Instant::now();
        tab.prs_loaded = Some(asked);
        {
            let _paused = test_pause::set(Some(Pause::Metered));
            tab.follow_network(&ctx);
        }
        let while_paused = (tab.avatars_hinted, tab.prs_loaded);
        {
            let _back = test_pause::set(None);
            tab.follow_network(&ctx);
        }
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(while_paused, (true, Some(asked)));
        // No forge here: nothing is asked, but what was asked before counts no more.
        assert!(!tab.avatars_hinted);
        assert_ne!(tab.prs_loaded, Some(asked));
    }

    /// When the repository was last fetched comes from `FETCH_HEAD`, and from a fetch that
    /// went through.
    #[test]
    fn the_last_fetch_is_read_from_fetch_head() {
        let dir = repo("fetch-head");
        let none = last_fetched(&dir);
        let _ = std::fs::write(dir.join(".git/FETCH_HEAD"), "");
        let some = last_fetched(&dir);
        let ctx = egui::Context::default();
        let tab = RepoTab::new_deferred(dir.clone(), &ctx);
        let before = tab.fetched_ago();
        tab.note_fetched();
        let after = tab.fetched_ago();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(none, None);
        assert!(some.is_some());
        assert_eq!(before, None);
        assert!(after.is_some_and(|d| d < Duration::from_secs(5)));
    }
}
