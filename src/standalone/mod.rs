//! Explicit native-file tools, independent of repository tabs and connection profiles.
mod cli;
mod directory;
mod files;
mod model;
mod permissions;
pub(crate) mod registration;
mod save;
mod ui;
#[cfg(any(windows, test))]
mod windows_metadata;

pub use cli::{Invocation, USAGE, parse};

pub fn run(invocation: Invocation, options: eframe::NativeOptions) -> Result<u8, String> {
    if !matches!(invocation, Invocation::Diff(_) | Invocation::Merge(_)) {
        return Err("Expected a standalone file tool".into());
    }
    let outcome = std::sync::Arc::new(std::sync::atomic::AtomicU8::new(1));
    let result = std::sync::Arc::clone(&outcome);
    eframe::run_native(
        "gitcraft-file-tools",
        options,
        Box::new(move |cc| Ok(Box::new(ui::ToolApp::new(invocation, result, &cc.egui_ctx)))),
    )
    .map_err(|e| e.to_string())?;
    Ok(outcome.load(std::sync::atomic::Ordering::Acquire))
}
