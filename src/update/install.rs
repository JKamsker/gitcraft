//! Downloading a release into a folder of its own ("parking" it), and putting it in place of
//! this copy of gitcraft. That's a rename, which Windows allows of a running program (not an
//! overwrite): the running one moves aside, to be kept as `<name>.previous` (see `previous`),
//! and the parked one takes its name.
//! The window then closes, having saved its state, and the new version starts.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::Duration;

use super::previous::{self, Previous, aside, remove};
use super::witness::{DownloadFailure, Failure, PlacementFailure};
use super::{REPO, Release};

/// A release's download is refused past this size.
const MAX_DOWNLOAD: u64 = 300 << 20;
/// Left in a parked release's folder once it is complete.
const READY: &str = "ready";

/// What a download of a release replaces.
enum Layout {
    /// A program file of its own (Windows, or Linux from the .tar.gz), replaced by the archive's.
    Program(PathBuf),
    /// The AppImage gitcraft runs from, replaced by the new AppImage.
    AppImage(PathBuf),
    /// The macOS app bundle, replaced by the new bundle.
    Bundle(PathBuf),
}

impl Layout {
    /// This copy of gitcraft's. Builds of `cargo run` are not updated, by a release they don't
    /// come from.
    fn get() -> Option<Layout> {
        if cfg!(debug_assertions) {
            return None;
        }
        if let Some(appimage) = crate::desktop_entry::appimage() {
            return Some(Layout::AppImage(appimage));
        }
        let exe = std::env::current_exe().ok()?;
        if cfg!(target_os = "macos") {
            exe.ancestors()
                .find(|p| p.extension().is_some_and(|e| e == "app"))
                .map(|bundle| Layout::Bundle(bundle.to_owned()))
        } else {
            Some(Layout::Program(exe))
        }
    }

    fn path(&self) -> &Path {
        match self {
            Layout::Program(p) | Layout::AppImage(p) | Layout::Bundle(p) => p,
        }
    }

    /// The file of release `tag` that it is updated from, named as the release workflow names
    /// them.
    fn asset(&self, tag: &str) -> Option<String> {
        let x64 = cfg!(target_arch = "x86_64");
        let target = match self {
            Layout::Program(_) if cfg!(windows) && x64 => "windows-x86_64.zip",
            Layout::Program(_) if cfg!(target_os = "linux") && x64 => "linux-x86_64.tar.gz",
            Layout::AppImage(_) if x64 => "linux-x86_64.AppImage",
            Layout::Bundle(_) => "macos-universal.zip",
            _ => return None,
        };
        Some(format!("gitcraft-{tag}-{target}"))
    }

    /// Where the new one is among what `asset` unpacks to.
    fn unpacked(&self, tag: &str, asset: &str) -> PathBuf {
        match self {
            Layout::Program(_) if cfg!(windows) => {
                PathBuf::from(format!("gitcraft-{tag}-windows-x86_64")).join("gitcraft.exe")
            }
            Layout::Program(_) => {
                PathBuf::from(format!("gitcraft-{tag}-linux-x86_64")).join("gitcraft")
            }
            Layout::AppImage(_) => PathBuf::from(asset),
            Layout::Bundle(_) => PathBuf::from("gitcraft.app"),
        }
    }
}

/// Whether this copy of gitcraft can put a new version in its place: not one a package manager
/// installed into a system folder, or one in a folder of the administrator's, or a macOS app
/// run from its disk image.
pub(super) fn can_update() -> bool {
    Layout::get().is_some_and(|layout| {
        let Some(dir) = layout.path().parent() else {
            return false;
        };
        let probe = dir.join(format!(".gitcraft-update-{}", std::process::id()));
        let writable = fs::File::create_new(&probe).is_ok();
        let _ = fs::remove_file(&probe);
        writable
    })
}

/// Where releases are parked: in the portable folder when gitcraft runs portable (nothing is
/// written outside it), else on Windows the local app data, not the roaming one that travels
/// with the profile.
fn parking() -> Option<PathBuf> {
    if let Some(dir) = crate::portable::portable_dir() {
        Some(dir.join("update"))
    } else if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .filter(|d| d.is_absolute())
            .map(|d| d.join("gitcraft").join("update"))
    } else {
        crate::portable::data_dir().map(|d| d.join("update"))
    }
}

/// One download at a time: another look (the settings changed) waits for the one running.
static DOWNLOADING: Mutex<()> = Mutex::new(());

/// `release`, downloaded and unpacked into a folder of its own unless it already is. Returns
/// the new version in there.
pub(super) fn prefetch(release: &Release) -> Result<PathBuf, String> {
    prefetch_witnessed(release).map_err(Failure::into_raw)
}

pub(super) fn prefetch_witnessed(release: &Release) -> Result<PathBuf, Failure> {
    let _one = DOWNLOADING.lock().unwrap_or_else(|e| e.into_inner());
    let layout = Layout::get().ok_or_else(not_updated)?;
    let tag = &release.tag;
    let asset = layout.asset(tag).ok_or_else(no_build)?;
    let parking = parking().ok_or_else(no_download_folder)?;
    let ready = parking.join(tag);
    let unpacked = layout.unpacked(tag, &asset);
    if ready.join(READY).is_file() && ready.join(&unpacked).exists() {
        return Ok(ready.join(unpacked));
    }
    // Only this one is kept: not an older one, nor a download that stopped halfway.
    if let Ok(entries) = fs::read_dir(&parking) {
        for entry in entries.flatten() {
            remove(&entry.path());
        }
    }
    // Complete only once renamed to its tag.
    let part = parking.join(format!("{tag}.part"));
    fs::create_dir_all(&part).map_err(|e| Failure::Opaque(format!("{}: {e}", part.display())))?;
    let url = format!("{REPO}/releases/download/{tag}/{asset}");
    let file = part.join(&asset);
    download_witnessed(&url, &file)?;
    if !matches!(layout, Layout::AppImage(_)) {
        unpack_witnessed(&file, &part)?;
        let _ = fs::remove_file(&file);
    }
    let new = part.join(&unpacked);
    let complete = match layout {
        Layout::Bundle(_) => new.join("Contents/MacOS/gitcraft").is_file(),
        _ => fs::metadata(&new).is_ok_and(|m| m.is_file() && m.len() > 0),
    };
    if !complete {
        return Err(asset_missing(asset));
    }
    #[cfg(unix)]
    if !matches!(layout, Layout::Bundle(_)) {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&new, fs::Permissions::from_mode(0o755))
            .map_err(|e| Failure::Opaque(format!("{}: {e}", new.display())))?;
    }
    fs::write(part.join(READY), &release.version).map_err(|e| Failure::Opaque(e.to_string()))?;
    remove(&ready);
    fs::rename(&part, &ready).map_err(|e| Failure::Opaque(format!("{}: {e}", ready.display())))?;
    Ok(ready.join(unpacked))
}

fn download(url: &str, to: &Path) -> Result<(), String> {
    download_witnessed(url, to).map_err(Failure::into_raw)
}

fn download_witnessed(url: &str, to: &Path) -> Result<(), Failure> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(15))
        .timeout_read(Duration::from_secs(60))
        .user_agent("gitcraft")
        .build();
    let failed =
        |e: &dyn std::fmt::Display| download_failure(url, DownloadFailure::Opaque(e.to_string()));
    let resp = agent.get(url).call().map_err(|e| failed(&e))?;
    let mut file = fs::File::create(to).map_err(|e| failed(&e))?;
    let copied = std::io::copy(&mut resp.into_reader().take(MAX_DOWNLOAD + 1), &mut file)
        .map_err(|e| failed(&e))?;
    check_download_size(url, copied)?;
    file.sync_all().map_err(|e| failed(&e))
}

/// Unpacks `archive` into `dir`: with `ditto` on macOS, which keeps an app bundle's signature
/// whole, `tar` elsewhere.
fn unpack(archive: &Path, dir: &Path) -> Result<(), String> {
    unpack_witnessed(archive, dir).map_err(Failure::into_raw)
}

fn unpack_witnessed(archive: &Path, dir: &Path) -> Result<(), Failure> {
    let mut cmd = if cfg!(target_os = "macos") {
        let mut cmd = Command::new("ditto");
        cmd.args(["-x", "-k"]).arg(archive).arg(dir);
        cmd
    } else {
        // Windows' own tar, which also opens zip files (Git's GNU tar earlier on the PATH
        // doesn't).
        let program = if cfg!(windows) {
            std::env::var_os("SystemRoot")
                .map(|root| PathBuf::from(root).join("System32").join("tar.exe"))
                .unwrap_or_else(|| PathBuf::from("tar.exe"))
        } else {
            PathBuf::from("tar")
        };
        let mut cmd = Command::new(program);
        cmd.arg("-xf").arg(archive).arg("-C").arg(dir);
        cmd
    };
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let out = cmd.output().map_err(|e| unpack_failure(e.to_string()))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(unpack_failure(
            String::from_utf8_lossy(&out.stderr).trim().to_owned(),
        ))
    }
}

/// Puts the parked version `new` in place of this copy of gitcraft, keeping this one to go
/// back to. Returns what to start.
pub fn install(new: &Path) -> Result<PathBuf, String> {
    install_witnessed(new).map_err(Failure::into_raw)
}

pub fn install_witnessed(new: &Path) -> Result<PathBuf, Failure> {
    let layout = Layout::get().ok_or_else(not_updated)?;
    let target = layout.path();
    previous::replace_keeping_witnessed(target, new, super::CURRENT, move_into_witnessed)?;
    if let Some(parking) = parking() {
        remove(&parking);
    }
    Ok(target.to_owned())
}

/// The version an update replaced, kept to go back to, if this copy can go back to it.
pub fn previous() -> Option<Previous> {
    let layout = Layout::get()?;
    can_update()
        .then(|| previous::find(layout.path()))
        .flatten()
        // The same version as this one (installed again by hand): nothing to go to.
        .filter(|p| p.version.as_deref() != Some(super::CURRENT))
}

/// Puts the version kept by the last update back in place of this one ("Go back to 0.3.3"),
/// keeping this one in its stead. Returns what to start, and the version gone back to.
pub fn go_back() -> Result<(PathBuf, Previous), String> {
    go_back_witnessed().map_err(Failure::into_raw)
}

pub fn go_back_witnessed() -> Result<(PathBuf, Previous), Failure> {
    let layout = Layout::get().ok_or_else(not_updated)?;
    let target = layout.path();
    let previous = previous::swap_back_witnessed(target, super::CURRENT)?;
    // A newer release parked for this version isn't one to offer the one gone back to.
    if let Some(parking) = parking() {
        remove(&parking);
    }
    Ok((target.to_owned(), previous))
}

/// A rename, or a copy when the parking is on another drive than gitcraft.
fn move_into(from: &Path, to: &Path) -> Result<(), String> {
    move_into_witnessed(from, to).map_err(PlacementFailure::into_raw)
}

fn move_into_witnessed(from: &Path, to: &Path) -> Result<(), PlacementFailure> {
    if fs::rename(from, to).is_ok() {
        return Ok(());
    }
    if from.is_file() {
        return fs::copy(from, to)
            .map(|_| ())
            .map_err(|e| PlacementFailure::Opaque(e.to_string()));
    }
    let copied = Command::new("ditto")
        .arg(from)
        .arg(to)
        .status()
        .map_err(|e| PlacementFailure::Opaque(e.to_string()))?;
    check_copy_status(copied.success(), || remove(to))
}

/// Removes a version an update moved aside and couldn't keep (see `previous`). It may still be
/// closing: tried for a while.
pub fn clean_up() {
    let Some(layout) = Layout::get() else { return };
    let old = aside(layout.path());
    if !old.exists() {
        return;
    }
    std::thread::spawn(move || {
        for _ in 0..30 {
            remove(&old);
            if !old.exists() {
                return;
            }
            std::thread::sleep(Duration::from_secs(1));
        }
    });
}

/// What to start once the window closed.
static RELAUNCH: Mutex<Option<PathBuf>> = Mutex::new(None);

/// Starts `path` once the window closed (and saved the open tabs, which it then opens).
pub fn relaunch_after_exit(path: PathBuf) {
    *RELAUNCH.lock().unwrap_or_else(|e| e.into_inner()) = Some(path);
}

/// Starts the version an update put in place, if there is one.
pub fn relaunch() {
    let Some(path) = RELAUNCH.lock().unwrap_or_else(|e| e.into_inner()).take() else {
        return;
    };
    let mut cmd = if cfg!(target_os = "macos") {
        // A new instance of the bundle: this one is still running, for a moment.
        let mut cmd = Command::new("open");
        cmd.arg("-n").arg(&path);
        cmd
    } else {
        Command::new(&path)
    };
    let _ = cmd.spawn();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_running_version_moves_aside_next_to_itself() {
        assert_eq!(
            aside(Path::new("/opt/x/gitcraft.app")),
            Path::new("/opt/x/gitcraft.app.old")
        );
        assert_eq!(
            aside(Path::new("/opt/x/gitcraft")),
            Path::new("/opt/x/gitcraft.old")
        );
    }

    #[test]
    fn release_files_as_the_workflow_names_them() {
        let program = Layout::Program(PathBuf::from("gitcraft"));
        let asset = program.asset("v0.3.4-dev.7");
        if cfg!(all(windows, target_arch = "x86_64")) {
            assert_eq!(
                asset.as_deref(),
                Some("gitcraft-v0.3.4-dev.7-windows-x86_64.zip")
            );
            assert_eq!(
                program.unpacked("v0.3.4-dev.7", ""),
                Path::new("gitcraft-v0.3.4-dev.7-windows-x86_64").join("gitcraft.exe")
            );
        }
        let bundle = Layout::Bundle(PathBuf::from("gitcraft.app"));
        assert_eq!(
            bundle.asset("v1.0.0").as_deref(),
            Some("gitcraft-v1.0.0-macos-universal.zip")
        );
    }
}

fn not_updated() -> Failure {
    Failure::NotUpdated {
        raw: "This copy of gitcraft isn't updated".to_owned(),
    }
}
fn no_build() -> Failure {
    Failure::NoBuild {
        raw: "The releases have no build for this system".to_owned(),
    }
}
fn no_download_folder() -> Failure {
    Failure::NoDownloadFolder {
        raw: "No folder to download the update into".to_owned(),
    }
}
fn asset_missing(asset: String) -> Failure {
    Failure::AssetMissing {
        raw: format!("{asset} holds no gitcraft"),
        asset,
    }
}
fn download_failure(url: &str, failure: DownloadFailure) -> Failure {
    Failure::Download {
        raw: format!("Could not download {url}: {}", failure.raw()),
        url: url.to_owned(),
        failure,
    }
}
// Actual post-copy guard. Tests call this pure guard without downloading/copying anything.
fn check_download_size(url: &str, copied: u64) -> Result<(), Failure> {
    if copied > MAX_DOWNLOAD {
        return Err(download_failure(
            url,
            DownloadFailure::TooBig {
                raw: "bigger than expected".to_owned(),
            },
        ));
    }
    Ok(())
}
fn unpack_failure(e: String) -> Failure {
    Failure::Unpack {
        raw: format!("Could not unpack the update: {e}"),
        detail: e,
    }
}
// Preserve the original failed-status cleanup, exactly once and only on failure.
fn check_copy_status(success: bool, cleanup: impl FnOnce()) -> Result<(), PlacementFailure> {
    success.then_some(()).ok_or_else(|| {
        cleanup();
        PlacementFailure::CopyFailed {
            raw: "Could not copy the new version".to_owned(),
        }
    })
}

#[cfg(test)]
#[path = "install/witness_tests.rs"]
mod witness_tests;
