//! Opt-in resident lifetime. Native callbacks only enqueue typed commands; the application
//! coordinator owns every window, launch and quit decision. Native objects stay on their
//! required thread and never enter `App` (whose deferred viewport callbacks are Send).
mod native;
pub(crate) mod watch;

use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Action {
    Show,
    Launch(Vec<String>),
    Quit,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Entry {
    pub title: String,
    pub action: Option<Action>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Availability {
    Disabled,
    Starting,
    Ready,
    Fallback(String),
}

static STATUS: Mutex<Availability> = Mutex::new(Availability::Disabled);
pub(crate) fn status() -> Availability {
    STATUS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
}

#[derive(Clone)]
pub(super) struct Events {
    tx: SyncSender<Action>,
    ctx: egui::Context,
    availability: Arc<Mutex<Availability>>,
}
impl Events {
    pub fn send(&self, action: Action) {
        // Bounded, nonblocking even if a native menu fires repeatedly while a picker runs.
        let _ = self.tx.try_send(action);
        self.ctx.request_repaint_of(egui::ViewportId::ROOT);
    }
    pub fn available(&self, status: Availability) {
        *self
            .availability
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = status;
        self.ctx.request_repaint_of(egui::ViewportId::ROOT);
    }
}

#[derive(Default)]
pub(crate) struct Resident {
    events: Option<(Events, Receiver<Action>)>,
    pub watcher: watch::Watcher,
    last_menu: Vec<Entry>,
    fallback_notified: bool,
}
impl Resident {
    pub fn sync(
        &mut self,
        enabled: bool,
        recent: &[std::path::PathBuf],
        text: &str,
        ctx: &egui::Context,
    ) -> Vec<Action> {
        if !enabled {
            if self.events.take().is_some() {
                native::stop();
                self.last_menu.clear();
                self.fallback_notified = false;
                *STATUS
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = Availability::Disabled;
            }
            return Vec::new();
        }
        let menu = menu(recent, text);
        if self.events.is_none() {
            let (tx, rx) = sync_channel(32);
            let events = Events {
                tx,
                ctx: ctx.clone(),
                availability: Arc::new(Mutex::new(Availability::Starting)),
            };
            native::start(menu.clone(), events.clone());
            self.events = Some((events, rx));
            self.last_menu = menu.clone();
        }
        native::poll();
        if menu != self.last_menu {
            native::update(menu.clone());
            self.last_menu = menu;
        }
        let Some((events, rx)) = &self.events else {
            return Vec::new();
        };
        let status = events
            .availability
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        *STATUS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = status.clone();
        if matches!(status, Availability::Fallback(_)) && !self.fallback_notified {
            self.fallback_notified = true;
            notify_fallback();
        }
        rx.try_iter().collect()
    }
}

pub(crate) fn menu(recent: &[std::path::PathBuf], text: &str) -> Vec<Entry> {
    let mut rows = vec![
        Entry {
            title: "Show gitcraft".into(),
            action: Some(Action::Show),
        },
        Entry {
            title: text.into(),
            action: None,
        },
    ];
    rows.extend(
        crate::os_shell::menu_items(recent)
            .into_iter()
            .map(|item| Entry {
                title: item.title,
                action: Some(Action::Launch(item.args)),
            }),
    );
    rows.push(Entry {
        title: "Quit gitcraft".into(),
        action: Some(Action::Quit),
    });
    rows
}

#[cfg(not(test))]
fn notify_fallback() {
    if let Ok(pool) = crate::tasks::pool() {
        let _ = pool.submit(Default::default(), crate::tasks::Priority::Background, None, Box::new(|| {
            let _ = notify_rust::Notification::new().appname("gitcraft").summary("gitcraft is still running")
                .body("This desktop has no available tray host. Open gitcraft from the launcher to show its window; use Quit gitcraft in its tab menu, or gitcraft --quit, to exit.").show();
        }));
    }
}
#[cfg(test)]
fn notify_fallback() {}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_commands_are_bounded_literal_and_status_is_not_an_action() -> Result<(), String> {
        let ctx = egui::Context::default();
        let (tx, rx) = sync_channel(2);
        let events = Events {
            tx,
            ctx,
            availability: Arc::new(Mutex::new(Availability::Starting)),
        };
        let literal = vec!["/repo with spaces/$(ignored);x".into()];
        events.send(Action::Launch(literal.clone()));
        events.send(Action::Quit);
        events.send(Action::Show);
        assert_eq!(
            rx.try_iter().collect::<Vec<_>>(),
            vec![Action::Launch(literal), Action::Quit]
        );
        let rows = menu(
            &["/repo with spaces/$(ignored);x".into()],
            "Offline: background checks paused",
        );
        assert_eq!(rows[0].action, Some(Action::Show));
        assert_eq!(rows[1].action, None);
        assert!(
            rows.iter()
                .any(|r| r.action == Some(Action::Launch(vec!["--clone".into()])))
        );
        assert_eq!(
            rows.last().and_then(|r| r.action.clone()),
            Some(Action::Quit)
        );
        Ok(())
    }
    #[test]
    fn opt_in_and_out_retire_the_callback_queue_without_hidden_startup_state() {
        let mut resident = Resident::default();
        let ctx = egui::Context::default();
        assert!(resident.sync(false, &[], "Paused", &ctx).is_empty());
        assert!(resident.events.is_none());
        assert!(resident.sync(true, &[], "Watching", &ctx).is_empty());
        let events = resident.events.as_ref().map(|e| e.0.clone());
        if let Some(events) = &events {
            events.send(Action::Show);
        }
        assert_eq!(
            resident.sync(true, &[], "Watching", &ctx),
            vec![Action::Show]
        );
        resident.sync(false, &[], "", &ctx);
        if let Some(events) = events {
            events.send(Action::Quit);
        }
        assert!(resident.sync(true, &[], "Watching", &ctx).is_empty());
    }
}
