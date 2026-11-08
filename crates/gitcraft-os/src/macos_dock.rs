//! AppKit objects stay on the application thread. Winit retains the NSMenu through its
//! existing delegate; each item retains its action target as representedObject as well.
use crate::{Link, Progress, dock::Presentation};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{DefinedClass, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSApplication, NSImageView, NSMenu, NSMenuItem, NSProgressIndicator, NSProgressIndicatorStyle,
    NSView,
};
use objc2_foundation::{
    MainThreadMarker, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString,
};
use std::cell::RefCell;

#[derive(Default)]
struct Actions {
    /// Literal arguments of this item's retained target; no sender or menu index is needed.
    arguments: Vec<String>,
}
impl Actions {
    fn activate(&self, _sender: Option<&AnyObject>) {
        crate::dock::activate(self.arguments.clone());
    }
}
define_class!(
    // SAFETY: NSObject has no subclassing requirements. Ivars are initialized before init;
    // selectors execute on the main thread. There is no custom Drop implementation.
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = Actions]
    struct GitcraftDockActions;
    unsafe impl NSObjectProtocol for GitcraftDockActions {}
    impl GitcraftDockActions {
        #[unsafe(method(activateDockItem:))]
        fn activate(&self, sender: Option<&AnyObject>) {
            // AppKit's documented Dock dispatch uses a nil sender.
            self.ivars().activate(sender);
        }
    }
);

pub fn set_menu(recent: &[Link], tasks: &[Link]) -> Result<(), String> {
    let mtm = MainThreadMarker::new().ok_or("Dock menus require the application thread")?;
    let menu = NSMenu::new(mtm);
    menu.setAutoenablesItems(false);
    for (index, link) in recent.iter().chain(tasks).enumerate() {
        if index == recent.len() && index != 0 {
            menu.addItem(&NSMenuItem::separatorItem(mtm));
        }
        // Each item has a distinct retained target: Dock dispatch does not supply its sender.
        // SAFETY: this no-requirement NSObject subclass has fully initialized Rust ivars.
        let target = GitcraftDockActions::alloc(mtm).set_ivars(Actions {
            arguments: link.args.clone(),
        });
        let target: Retained<GitcraftDockActions> = unsafe { msg_send![super(target), init] };
        // SAFETY: target implements the exact action selector, and remains retained by each
        // item's representedObject for as long as AppKit or winit holds that menu item.
        let item = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(mtm),
                &NSString::from_str(&link.title),
                Some(sel!(activateDockItem:)),
                &NSString::from_str(""),
            )
        };
        item.setToolTip(Some(&NSString::from_str(&link.tooltip)));
        unsafe {
            item.setTarget(Some(&target));
            item.setRepresentedObject(Some(&target));
        }
        menu.addItem(&item);
    }
    // SAFETY: the pointer is a live NSMenu created on this main thread. The narrow backend
    // setter retains it; objc2 versions wrap the same Objective-C NSMenu runtime class.
    unsafe {
        winit::platform::macos::set_dock_menu(Retained::as_ptr(&menu).cast_mut().cast());
    }
    Ok(())
}

thread_local! { static BAR: RefCell<Option<Retained<NSProgressIndicator>>> = const { RefCell::new(None) }; }

pub fn set_progress(progress: Progress) -> Result<(), String> {
    let mtm = MainThreadMarker::new().ok_or("Dock progress requires the application thread")?;
    let app = NSApplication::sharedApplication(mtm);
    let tile = app.dockTile();
    let display = Presentation::from(progress);
    if let Some(old) = BAR.with(|bar| bar.borrow_mut().take()) {
        // SAFETY: the sender is optional and this indicator belongs to the main thread.
        unsafe {
            old.stopAnimation(None);
        }
    }
    tile.setBadgeLabel(display.badge.map(NSString::from_str).as_deref());
    if display.fraction.is_none() && !display.indeterminate {
        tile.setContentView(None);
    } else {
        let size = tile.size();
        let bounds = NSRect::new(NSPoint::new(0.0, 0.0), size);
        let view = NSView::initWithFrame(NSView::alloc(mtm), bounds);
        let icon = NSImageView::initWithFrame(NSImageView::alloc(mtm), bounds);
        icon.setImage(app.applicationIconImage().as_deref());
        let bar = NSProgressIndicator::initWithFrame(
            NSProgressIndicator::alloc(mtm),
            NSRect::new(
                NSPoint::new(size.width * 0.08, size.height * 0.06),
                NSSize::new(size.width * 0.84, size.height * 0.14),
            ),
        );
        bar.setStyle(NSProgressIndicatorStyle::Bar);
        bar.setMinValue(0.0);
        bar.setMaxValue(1.0);
        bar.setIndeterminate(display.indeterminate);
        bar.setDoubleValue(f64::from(display.fraction.unwrap_or(0.0)));
        // SAFETY: both subviews are live NSViews on this same application thread.
        unsafe {
            view.addSubview(&icon);
            view.addSubview(&bar);
            if display.indeterminate {
                bar.startAnimation(None);
            }
        }
        tile.setContentView(Some(&view));
        BAR.with(|current| *current.borrow_mut() = Some(bar));
    }
    tile.display();
    Ok(())
}

/// NSDockTile snapshots its view. Refresh animation during the existing model progress tick.
pub fn refresh() {
    if let Some(mtm) = MainThreadMarker::new() {
        NSApplication::sharedApplication(mtm).dockTile().display();
    }
}

#[cfg(test)]
mod tests {
    use super::Actions;
    #[test]
    fn nil_sender_callbacks_dispatch_each_targets_literal_argv_without_native_objects() {
        let entries = vec![
            vec!["/Users/a/repo with spaces;$(literal)".into()],
            vec!["--clone".into()],
            vec!["--open".into()],
        ];
        let targets: Vec<_> = entries
            .iter()
            .cloned()
            .map(|arguments| Actions { arguments })
            .collect();
        assert!(crate::take_native_dock_requests().is_empty());
        for target in &targets {
            // Same Rust callback reached by the nullable Objective-C selector. No NSMenuItem,
            // Foundation allocation, NSApplication or event-loop/native dispatch is involved.
            target.activate(None);
        }
        assert_eq!(
            crate::take_native_dock_requests(),
            entries.into_iter().map(Ok).collect::<Vec<_>>()
        );
        // An old target retains its own action after other targets/menu generations exist.
        targets[0].activate(None);
        assert_eq!(
            crate::take_native_dock_requests(),
            [Ok(targets[0].arguments.clone())]
        );
    }
}
