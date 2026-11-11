#[cfg(test)]
use super::{Entry, Events};

#[cfg(test)]
pub(super) fn start(_: Vec<Entry>, events: Events) {
    events.available(super::Availability::Ready);
}
#[cfg(test)]
pub(super) fn update(_: Vec<Entry>) {}
#[cfg(test)]
pub(super) fn stop() {}

#[cfg(all(not(test), any(windows, target_os = "macos")))]
#[path = "native_desktop.rs"]
mod platform;
#[cfg(all(not(test), target_os = "linux"))]
#[path = "native_linux.rs"]
mod platform;
#[cfg(all(not(test), target_os = "linux"))]
pub(super) use platform::poll;
#[cfg(not(test))]
pub(super) use platform::{start, stop, update};
#[cfg(any(test, not(target_os = "linux")))]
pub(super) fn poll() {}
