//! Owns update discovery requests, settings identity, and completion status.

use super::*;

#[derive(Default)]
pub(super) struct Discovery {
    rx: Option<std::sync::mpsc::Receiver<Result<Option<update::Found>, String>>>,
    looked: Option<(bool, update::Channel, bool)>,
    error: Option<String>,
    checked: bool,
}

impl Discovery {
    pub(super) fn invalidate(&mut self) {
        self.looked = None;
    }

    fn start(
        &mut self,
        releases: update::Channel,
        download: bool,
        skipped: String,
        pinned: String,
        ctx: &egui::Context,
    ) {
        let (tx, rx) = std::sync::mpsc::channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(update::look(releases, download, &skipped, &pinned));
            ctx.request_repaint();
        });
        self.rx = Some(rx);
    }

    /// A settings change retires the old receiver before launching another request.
    fn refresh(
        &mut self,
        want: (bool, update::Channel, bool),
        skipped: &str,
        pinned: &str,
        ctx: &egui::Context,
    ) -> bool {
        if self.looked == Some(want) || !crate::network::background_may_run() {
            return false;
        }
        self.looked = Some(want);
        self.rx = None;
        let (check, releases, download) = want;
        if check {
            self.start(releases, download, skipped.into(), pinned.into(), ctx);
        }
        true
    }

    /// None means no successful completion; Some(None) is a completed empty result.
    fn take_answer(&mut self) -> Option<Option<update::Found>> {
        let result = self.rx.as_ref()?.try_recv().ok()?;
        self.rx = None;
        match result {
            Ok(found) => {
                self.checked = true;
                self.error = None;
                Some(found)
            }
            Err(e) => {
                self.error = Some(e);
                None
            }
        }
    }

    fn check_now(&mut self, settings: &Settings, pinned: &str, ctx: &egui::Context) -> bool {
        if self.rx.is_some() {
            return false;
        }
        self.start(
            settings.update_channel,
            settings.download_updates,
            String::new(),
            pinned.into(),
            ctx,
        );
        self.error = None;
        true
    }

    fn status(&self, pinned: &str, found: Option<&update::Found>) -> crate::settings::UpdateCheck {
        use crate::settings::UpdateCheck;
        if !pinned.is_empty() {
            UpdateCheck::Pinned(pinned.into())
        } else if self.rx.is_some() {
            UpdateCheck::Checking
        } else if let Some(found) = found {
            UpdateCheck::Found(found.release.version.clone())
        } else if let Some(e) = &self.error {
            UpdateCheck::Failed(format!("Couldn't look: {e}"))
        } else if self.checked {
            UpdateCheck::UpToDate
        } else {
            UpdateCheck::NotYet
        }
    }
}

impl App {
    pub(super) fn poll_update(&mut self, ctx: &egui::Context) {
        let s = &self.settings;
        if self.update_discovery.refresh(
            (s.check_updates, s.update_channel, s.download_updates),
            &self.skipped_version,
            &self.pinned_version,
            ctx,
        ) {
            self.update = None;
        }
        if let Some(found) = self.update_discovery.take_answer() {
            self.update = found;
        }
    }

    pub(super) fn check_update_now(&mut self, ctx: &egui::Context) {
        if self
            .update_discovery
            .check_now(&self.settings, &self.pinned_version, ctx)
        {
            // Asked for: the bar shows what is found, even after "Not now".
            self.update_later = false;
        }
    }

    pub(super) fn update_check(&self) -> crate::settings::UpdateCheck {
        self.update_discovery
            .status(&self.pinned_version, self.update.as_ref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completion_is_consumed_once_and_success_clears_the_retained_error() -> Result<(), String> {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut discovery = Discovery {
            rx: Some(rx),
            ..Default::default()
        };
        tx.send(Err("opaque failure".into()))
            .map_err(|e| e.to_string())?;
        assert!(discovery.take_answer().is_none());
        assert_eq!(discovery.error.as_deref(), Some("opaque failure"));
        assert!(!discovery.checked);
        assert!(discovery.rx.is_none());
        let (tx, rx) = std::sync::mpsc::channel();
        discovery.rx = Some(rx);
        tx.send(Ok(None)).map_err(|e| e.to_string())?;
        assert!(matches!(discovery.take_answer(), Some(None)));
        assert!(discovery.checked);
        assert!(discovery.error.is_none());
        assert!(discovery.take_answer().is_none());
        Ok(())
    }

    #[test]
    fn replacing_a_receiver_discards_the_retired_request_without_affecting_the_new_one()
    -> Result<(), String> {
        let (old_tx, old_rx) = std::sync::mpsc::channel();
        let mut discovery = Discovery {
            rx: Some(old_rx),
            ..Default::default()
        };
        let (new_tx, new_rx) = std::sync::mpsc::channel();
        discovery.rx = Some(new_rx);
        assert!(old_tx.send(Err("retired".into())).is_err());
        assert!(discovery.take_answer().is_none());
        new_tx.send(Ok(None)).map_err(|e| e.to_string())?;
        assert!(matches!(discovery.take_answer(), Some(None)));
        assert!(discovery.error.is_none());
        Ok(())
    }
}
