//! Own one GetURL Apple-event handler, without replacing winit's NSApplication delegate.
use objc2::rc::Retained;
use objc2::{MainThreadOnly, define_class, msg_send, sel};
use objc2_foundation::{
    MainThreadMarker, NSAppleEventDescriptor, NSAppleEventManager, NSObject, NSObjectProtocol,
};
use std::cell::Cell;

const INTERNET: u32 = u32::from_be_bytes(*b"GURL");
const GET_URL: u32 = u32::from_be_bytes(*b"GURL");
const DIRECT_OBJECT: u32 = u32::from_be_bytes(*b"----");
thread_local! { static INSTALLED: Cell<bool> = const { Cell::new(false) }; }

define_class!(
    // SAFETY: NSObject has no subclassing requirements. The handler is main-thread-only,
    // has no ivars or Drop implementation, and its selector has the Apple-event signature.
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    struct GitcraftUrlHandler;
    unsafe impl NSObjectProtocol for GitcraftUrlHandler {}
    impl GitcraftUrlHandler {
        #[unsafe(method(receiveURL:withReplyEvent:))]
        fn receive_url(&self, event: &NSAppleEventDescriptor, _reply: &NSAppleEventDescriptor) {
            // SAFETY: AEKeyword is a 32-bit OSType. The exact Foundation signature
            // returns an owned/autoreleased descriptor; objc2 retains that return value.
            let direct: Option<Retained<NSAppleEventDescriptor>> = unsafe {
                msg_send![event, paramDescriptorForKeyword: DIRECT_OBJECT]
            };
            let value = direct.and_then(|d| d.stringValue());
            let result = match value {
                Some(value) if value.length() <= 4096 => Ok(value.to_string()),
                Some(_) => Err("The gitcraft link is too long (maximum 4096 bytes)".into()),
                None => Err("The native URL event had no URL string".into()),
            };
            super::native_urls::receive(result);
        }
    }
);

/// A process-lifetime handler installed on the main thread before eframe runs its event loop.
/// Dropping it removes only the event class/ID this guard installed.
pub struct NativeUrlHandler {
    _handler: Retained<GitcraftUrlHandler>,
}

impl NativeUrlHandler {
    pub fn install() -> Result<Self, String> {
        let mtm = MainThreadMarker::new().ok_or("Native URL events require the main thread")?;
        if INSTALLED.with(|i| i.replace(true)) {
            return Err("The native URL handler is already installed".into());
        }
        // SAFETY: NSObject init is valid for this no-ivar subclass. The manager retains a
        // handler implementing exactly receiveURL:withReplyEvent:; this guard also owns it
        // until after the event loop exits. No NSApplication delegate is read or modified.
        let handler: Retained<GitcraftUrlHandler> =
            unsafe { msg_send![GitcraftUrlHandler::alloc(mtm), init] };
        let manager = NSAppleEventManager::sharedAppleEventManager();
        unsafe {
            let (): () = msg_send![&manager,
                setEventHandler: &*handler,
                andSelector: sel!(receiveURL:withReplyEvent:),
                forEventClass: INTERNET,
                andEventID: GET_URL
            ];
        }
        Ok(Self { _handler: handler })
    }
}

impl Drop for NativeUrlHandler {
    fn drop(&mut self) {
        // SAFETY: both event identifiers are the same u32 values registered above.
        let manager = NSAppleEventManager::sharedAppleEventManager();
        unsafe {
            let (): () =
                msg_send![&manager, removeEventHandlerForEventClass: INTERNET, andEventID: GET_URL];
        }
        INSTALLED.with(|i| i.set(false));
    }
}
