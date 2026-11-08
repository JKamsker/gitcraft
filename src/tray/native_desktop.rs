//! Main-thread native objects. Called by the root's running event loop, never AppCreator.
use super::super::{Action, Availability, Entry, Events};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tray_icon::{
    TrayIcon, TrayIconBuilder,
    menu::{Menu, MenuEvent, MenuItem},
};

thread_local! { static NATIVE: RefCell<Option<TrayIcon>> = const { RefCell::new(None) }; }
static IDS: AtomicU64 = AtomicU64::new(0);
fn build_menu(
    rows: Vec<Entry>,
    actions: &Mutex<HashMap<tray_icon::menu::MenuId, Action>>,
) -> Result<Menu, String> {
    let menu = Menu::new();
    let mut next = HashMap::new();
    for row in rows {
        let id = format!("gitcraft-tray-{}", IDS.fetch_add(1, Ordering::Relaxed));
        let item = MenuItem::with_id(id, row.title, row.action.is_some(), None);
        if let Some(action) = row.action {
            next.insert(item.id().clone(), action);
        }
        menu.append(&item).map_err(|e| e.to_string())?;
    }
    *actions
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = next;
    Ok(menu)
}
thread_local! { static ACTIONS: Arc<Mutex<HashMap<tray_icon::menu::MenuId, Action>>> = Arc::default(); }

pub(in crate::tray) fn start(rows: Vec<Entry>, events: Events) {
    let result = ACTIONS.with(|actions| {
        let menu = build_menu(rows, actions)?;
        let image =
            image::load_from_memory(include_bytes!("../../assets/app-icon/gitcraft-256.png"))
                .map_err(|e| e.to_string())?
                .resize_exact(32, 32, image::imageops::FilterType::Triangle)
                .to_rgba8();
        let icon =
            tray_icon::Icon::from_rgba(image.into_raw(), 32, 32).map_err(|e| e.to_string())?;
        let actions = Arc::clone(actions);
        let send = events.clone();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            let action = actions
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .get(&event.id)
                .cloned();
            if let Some(action) = action {
                send.send(action);
            }
        }));
        let icon = TrayIconBuilder::new()
            .with_id("gitcraft-resident")
            .with_icon(icon)
            .with_menu(Box::new(menu))
            .with_tooltip(crate::i18n::text(
                "interaction-control-gitcraft-repository-watcher",
            ))
            .build()
            .map_err(|e| e.to_string())?;
        NATIVE.with(|slot| *slot.borrow_mut() = Some(icon));
        Ok::<_, String>(())
    });
    events.available(match result {
        Ok(()) => Availability::Ready,
        Err(why) => Availability::Fallback(why),
    });
}
pub(in crate::tray) fn update(rows: Vec<Entry>) {
    ACTIONS.with(|actions| {
        if let Ok(menu) = build_menu(rows, actions) {
            NATIVE.with(|slot| {
                if let Some(icon) = slot.borrow().as_ref() {
                    icon.set_menu(Some(Box::new(menu)));
                    let _ = icon.set_tooltip(Some(crate::i18n::text(
                        "interaction-control-gitcraft-repository-watcher",
                    )));
                }
            });
        }
    });
}
pub(in crate::tray) fn stop() {
    NATIVE.with(|slot| {
        slot.borrow_mut().take();
    });
    ACTIONS.with(|actions| {
        actions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear()
    });
    MenuEvent::set_event_handler(None::<fn(MenuEvent)>);
}
