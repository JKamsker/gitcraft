//! Platform adapters for the native window border.

#[cfg(windows)]
#[allow(unsafe_code)]
pub(super) fn set_window_border(window: &impl raw_window_handle::HasWindowHandle, shown: bool) {
    use raw_window_handle::RawWindowHandle;
    use windows_sys::Win32::Graphics::Dwm::{
        DWMWA_BORDER_COLOR, DWMWA_COLOR_DEFAULT, DWMWA_COLOR_NONE, DwmSetWindowAttribute,
    };
    let Ok(handle) = window.window_handle() else {
        return;
    };
    let RawWindowHandle::Win32(win32) = handle.as_raw() else {
        return;
    };
    let color: u32 = if shown {
        DWMWA_COLOR_DEFAULT
    } else {
        DWMWA_COLOR_NONE
    };
    // SAFETY: `win32.hwnd` is this app's window, alive while `handle` borrows it for this
    // call. `color` is a u32 on the stack that outlives the call, and its exact size is
    // passed; Windows only reads it. A failure is an error code, left alone: the border just
    // stays.
    let _ = unsafe {
        DwmSetWindowAttribute(
            win32.hwnd.get() as _,
            DWMWA_BORDER_COLOR as u32,
            std::ptr::from_ref(&color).cast(),
            size_of::<u32>() as u32,
        )
    };
}

/// Change the native title bar without replacing the titled frame that rounds the corners.
#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
pub(super) fn set_window_border(window: &impl raw_window_handle::HasWindowHandle, shown: bool) {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSView, NSWindowButton, NSWindowStyleMask, NSWindowTitleVisibility};
    use raw_window_handle::RawWindowHandle;

    let Some(_main_thread) = MainThreadMarker::new() else {
        return;
    };
    let Ok(handle) = window.window_handle() else {
        return;
    };
    let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
        return;
    };
    // SAFETY: the borrowed window handle guarantees a live NSView, and the main-thread check
    // above ensures that AppKit is accessed only from the window's own thread.
    let view = unsafe { appkit.ns_view.cast::<NSView>().as_ref() };
    let Some(window) = view.window() else {
        return;
    };
    let mut style = window.styleMask();
    style.set(NSWindowStyleMask::FullSizeContentView, !shown);
    window.setStyleMask(style);
    window.setTitlebarAppearsTransparent(!shown);
    window.setTitleVisibility(if shown {
        NSWindowTitleVisibility::Visible
    } else {
        NSWindowTitleVisibility::Hidden
    });
    for kind in [
        NSWindowButton::CloseButton,
        NSWindowButton::MiniaturizeButton,
        NSWindowButton::ZoomButton,
    ] {
        if let Some(button) = window.standardWindowButton(kind) {
            button.setHidden(!shown);
        }
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
pub(super) fn set_window_border(_window: &impl raw_window_handle::HasWindowHandle, _shown: bool) {}
