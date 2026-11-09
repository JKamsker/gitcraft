//! StatusNotifierItem without GTK. Host availability is reported by KSNI, including a host
//! appearing/disappearing after startup. D-Bus setup runs in the existing bounded worker lane.
use super::super::{Action, Availability, Entry, Events};
use ksni::blocking::{Handle, TrayMethods};
use std::cell::RefCell;
use std::sync::mpsc::{Receiver, channel};

struct Item {
    rows: Vec<Entry>,
    events: Events,
    icon: Vec<ksni::Icon>,
}
impl ksni::Tray for Item {
    fn id(&self) -> String {
        "gitcraft".into()
    }
    fn title(&self) -> String {
        "gitcraft".into()
    }
    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        self.icon.clone()
    }
    fn activate(&mut self, _: i32, _: i32) {
        self.events.send(Action::Show);
    }
    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        self.rows
            .iter()
            .map(|row| {
                let action = row.action.clone();
                ksni::menu::StandardItem {
                    label: row.title.clone(),
                    enabled: action.is_some(),
                    activate: Box::new(move |item: &mut Self| {
                        if let Some(action) = &action {
                            item.events.send(action.clone());
                        }
                    }),
                    ..Default::default()
                }
                .into()
            })
            .collect()
    }
    fn watcher_online(&self) {
        self.events.available(Availability::Ready);
    }
    fn watcher_offline(&self, reason: ksni::OfflineReason) -> bool {
        self.events
            .available(Availability::Fallback(crate::i18n::message(
                "interaction-watch-no-host",
                &[("detail", format!("{reason:?}").into())],
            )));
        true // remain registered so a later tray host can restore the item
    }
}
enum State {
    Starting {
        rx: Receiver<Handle<Item>>,
        rows: Vec<Entry>,
        cancel: crate::tasks::Cancellation,
    },
    Running(Handle<Item>),
}
thread_local! { static NATIVE: RefCell<Option<State>> = const { RefCell::new(None) }; }
pub(in crate::tray) fn start(rows: Vec<Entry>, events: Events) {
    let (tx, rx) = channel();
    let cancel = crate::tasks::Cancellation::default();
    let work_cancel = cancel.clone();
    let initial = rows.clone();
    let failed = events.clone();
    let submitted = crate::tasks::pool().and_then(|pool| {
        pool.submit(
            cancel.clone(),
            crate::tasks::Priority::Background,
            None,
            Box::new(move || {
                let icon = image::load_from_memory(include_bytes!(
                    "../../assets/app-icon/gitcraft-256.png"
                ))
                .ok()
                .map(|image| {
                    let rgba = image
                        .resize_exact(32, 32, image::imageops::FilterType::Triangle)
                        .to_rgba8();
                    let argb = rgba
                        .pixels()
                        .flat_map(|p| [p[3], p[0], p[1], p[2]])
                        .collect();
                    ksni::Icon {
                        width: 32,
                        height: 32,
                        data: argb,
                    }
                })
                .into_iter()
                .collect();
                // Soft host failures reach watcher_offline; a missing session bus is a hard error.
                let sandboxed =
                    std::env::var_os("FLATPAK_ID").is_some() || std::env::var_os("SNAP").is_some();
                match (Item {
                    rows: initial,
                    events: events.clone(),
                    icon,
                })
                .assume_sni_available(true)
                .disable_dbus_name(sandboxed)
                .spawn()
                {
                    Ok(handle) => {
                        if work_cancel.is_cancelled() {
                            handle.shutdown();
                            return;
                        }
                        let mut availability = events
                            .availability
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        if *availability == Availability::Starting {
                            *availability = Availability::Ready;
                        }
                        drop(availability);
                        events.ctx.request_repaint_of(egui::ViewportId::ROOT);
                        if let Err(undelivered) = tx.send(handle) {
                            undelivered.0.shutdown();
                        }
                    }
                    Err(error) => events.available(Availability::Fallback(error.to_string())),
                }
            }),
        )
        .map_err(str::to_owned)
    });
    if let Err(error) = submitted {
        failed.available(Availability::Fallback(error));
        return;
    }
    NATIVE.with(|slot| *slot.borrow_mut() = Some(State::Starting { rx, rows, cancel }));
}
pub(in crate::tray) fn poll() {
    NATIVE.with(|slot| {
        let mut state = slot.borrow_mut();
        if let Some(State::Starting { rx, rows, .. }) = state.as_ref()
            && let Ok(handle) = rx.try_recv()
        {
            let rows = rows.clone();
            handle.update(move |item| item.rows = rows);
            *state = Some(State::Running(handle));
        }
    });
}
pub(in crate::tray) fn update(rows: Vec<Entry>) {
    poll();
    NATIVE.with(|slot| match slot.borrow_mut().as_mut() {
        Some(State::Starting { rows: pending, .. }) => *pending = rows,
        Some(State::Running(handle)) => {
            handle.update(move |item| item.rows = rows);
        }
        None => {}
    });
}
pub(in crate::tray) fn stop() {
    NATIVE.with(|slot| match slot.borrow_mut().take() {
        Some(State::Starting { cancel, .. }) => cancel.cancel(),
        Some(State::Running(handle)) => {
            handle.shutdown();
        }
        None => {}
    });
}
