//! Portable mode: a `gitcraft.portable` file or a `data` folder next to gitcraft keeps what it
//! stores (settings, open tabs, profiles and their git config files, the avatar cache,
//! downloaded updates and tools) in that `data` folder, instead of the user's app data. For a
//! copy on a USB stick, or on a machine where nothing may be written elsewhere. The system's
//! credential store is then used only when the settings say so (see [`credential_store`]).

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

/// The file next to gitcraft that turns portable mode on.
pub const MARKER: &str = "gitcraft.portable";
/// The folder next to gitcraft that turns it on too, and holds what is stored.
pub const DATA: &str = "data";

/// Where gitcraft keeps what it stores, as found on start.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Mode {
    /// In the user's app data (`%APPDATA%`, `~/.local/share`, `~/Library/Application Support`).
    Installed,
    /// In this folder next to gitcraft.
    Portable(PathBuf),
    /// Portable mode was asked for, but its folder can't be made or written to: the user's app
    /// data is used instead, and this says why.
    Unusable(String),
}

/// How this copy of gitcraft stores what it keeps, looked up once.
pub fn mode() -> &'static Mode {
    static MODE: OnceLock<Mode> = OnceLock::new();
    MODE.get_or_init(|| match program_dir() {
        Some(dir) => detect(&dir),
        None => Mode::Installed,
    })
}

/// The portable folder in use, if gitcraft runs portable.
pub fn portable_dir() -> Option<&'static Path> {
    match mode() {
        Mode::Portable(dir) => Some(dir),
        _ => None,
    }
}

/// The folder for gitcraft's own files: the portable one, or the user's app data.
pub fn data_dir() -> Option<PathBuf> {
    match portable_dir() {
        Some(dir) => Some(dir.to_owned()),
        None => eframe::storage_dir("gitcraft"),
    }
}

/// The file eframe saves the window's state in, when that isn't its own default place.
pub fn persistence_path() -> Option<PathBuf> {
    portable_dir().map(|dir| dir.join("app.ron"))
}

/// Whether a portable copy may keep sign-ins in the system's credential store (Credential
/// Manager, Keychain, Secret Service), as the settings say: off until they are applied.
static STORE_WANTED: AtomicBool = AtomicBool::new(false);

/// Whether the credential store is used: always when installed, as the settings say when
/// portable. Read from any thread.
pub fn credential_store() -> bool {
    credential_store_allowed(asked(), STORE_WANTED.load(Ordering::Relaxed))
}

/// Applies the setting, which counts only when portable mode was asked for (also when its
/// folder couldn't be used: the user still asked for nothing to be kept on this computer).
pub fn set_credential_store(wanted: bool) {
    STORE_WANTED.store(wanted, Ordering::Relaxed);
}

/// Whether portable mode was asked for, whether or not its folder could be used.
pub fn asked() -> bool {
    !matches!(mode(), Mode::Installed)
}

/// Whether the credential store is used: always when installed, as asked when portable.
pub(crate) fn credential_store_allowed(portable: bool, wanted: bool) -> bool {
    !portable || wanted
}

/// The folder gitcraft runs from, as the user sees it.
fn program_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    program_dir_of(&exe, crate::desktop_entry::appimage().as_deref())
}

/// The folder holding `exe`, or the AppImage it runs from, or (on macOS) the app bundle it is
/// in (`gitcraft.app/Contents/MacOS/gitcraft`): where the marker and the `data` folder go.
pub(crate) fn program_dir_of(exe: &Path, appimage: Option<&Path>) -> Option<PathBuf> {
    let program = appimage.unwrap_or(exe);
    let program = if cfg!(target_os = "macos") {
        bundle_of(program).unwrap_or(program)
    } else {
        program
    };
    program
        .parent()
        .filter(|dir| !dir.as_os_str().is_empty())
        .map(Path::to_owned)
}

/// The app bundle `exe` is the program of: `X.app/Contents/MacOS/<exe>`.
pub(crate) fn bundle_of(exe: &Path) -> Option<&Path> {
    let macos = exe.parent()?;
    let contents = macos.parent()?;
    let bundle = contents.parent()?;
    let is_bundle = macos.file_name()? == "MacOS"
        && contents.file_name()? == "Contents"
        && bundle
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("app"));
    is_bundle.then_some(bundle)
}

/// The mode for a gitcraft in `dir`. The `data` folder is made when only the marker is there.
/// A `data` folder alone counts only when it is empty or already holds gitcraft's state: an
/// unrelated `data` folder of the user's (in `~/Downloads`, say) doesn't turn it on.
pub(crate) fn detect(dir: &Path) -> Mode {
    let data = dir.join(DATA);
    let asked = dir.join(MARKER).is_file() || is_portable_data(&data);
    if !asked {
        return Mode::Installed;
    }
    // macOS runs a quarantined app from a random read-only copy, away from the marker.
    if dir
        .to_string_lossy()
        .replace('\\', "/")
        .contains("/AppTranslocation/")
    {
        return Mode::Unusable(TRANSLOCATED.to_owned());
    }
    match usable(&data) {
        Ok(()) => Mode::Portable(data),
        Err(e) => Mode::Unusable(e),
    }
}

/// Said when macOS runs the app from its quarantine copy (see [`detect`]).
const TRANSLOCATED: &str = "Portable mode is off: macOS runs this downloaded copy from a \
    temporary folder. Run xattr -dr com.apple.quarantine on gitcraft.app, then start it again.";

/// A `data` folder that is gitcraft's: empty, or holding the window's saved state.
fn is_portable_data(data: &Path) -> bool {
    if !data.is_dir() {
        return false;
    }
    data.join("app.ron").is_file()
        || std::fs::read_dir(data).is_ok_and(|mut entries| entries.next().is_none())
}

/// Whether gitcraft can keep its files in `data`: the folder is made if need be, and a file is
/// written to it and removed (a read-only stick or a locked-down folder can't take them).
fn usable(data: &Path) -> Result<(), String> {
    let refused = |e: std::io::Error| {
        format!(
            "Portable mode is off: gitcraft can't write to {} ({e}). Settings are kept in your \
             user's app data this time.",
            data.display()
        )
    };
    std::fs::create_dir_all(data).map_err(refused)?;
    let probe = data.join(format!(".gitcraft-write-test-{}", std::process::id()));
    std::fs::write(&probe, b"").map_err(refused)?;
    let _ = std::fs::remove_file(&probe);
    Ok(())
}

#[cfg(test)]
mod tests;
