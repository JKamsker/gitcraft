//! Application of a selected profile, config snapshots and their leases.

use super::{UserProfile, config_text, user_global_files};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Where the config file of profile `id` goes.
fn config_path(id: u64) -> Option<PathBuf> {
    crate::portable::data_dir().map(|d| profile_file(&d, id))
}

pub(super) fn profile_file(storage: &Path, id: u64) -> PathBuf {
    storage.join("profiles").join(format!("{id}.gitconfig"))
}

/// The file written last and what was in it, so that it is written again only when it changes.
static WRITTEN: Mutex<Option<(PathBuf, String)>> = Mutex::new(None);

/// Why the active profile's config couldn't be written, if it couldn't.
static ERROR: Mutex<Option<String>> = Mutex::new(None);

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Writes `text` to `path` through a file next to it, so git never reads half of it.
pub(super) fn write_file(path: &Path, text: &str) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let tmp = path.with_extension("gitconfig.tmp");
    std::fs::write(&tmp, text).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))
}

/// Has gitcraft's git commands go by `profile` from now on: writes its config file (when it
/// changed) and points git at it, or back at the user's own config when it sets nothing.
pub fn apply(profile: &UserProfile) {
    let mut context = crate::connections::Context {
        choices: profile.connections.clone(),
        ..Default::default()
    };
    if profile.pending_setup() {
        let message =
            "This imported profile needs fresh local account/key setup before use".to_owned();
        context.choices = Default::default();
        context.error = Some(message.clone());
        *lock(&ERROR) = Some(message);
        crate::connections::set(context);
        return;
    }
    let result = profile.connections.validate().and_then(|_| {
        if profile.sets_git() && std::env::current_exe().is_err() {
            return Err("Could not locate gitcraft's connection helpers".to_owned());
        }
        if !profile.sets_git() {
            return Ok(None);
        }
        config_path(profile.id)
            .ok_or_else(|| "No folder to keep the profile's git config in".to_owned())
            .and_then(|path| {
                let text = config_text(profile, &user_global_files());
                let mut written = lock(&WRITTEN);
                let fresh = written
                    .as_ref()
                    .is_some_and(|(p, t)| *p == path && *t == text)
                    && path.is_file();
                if !fresh {
                    write_file(&path, &text)?;
                    *written = Some((path.clone(), text.clone()));
                }
                static SESSION: std::sync::OnceLock<u128> = std::sync::OnceLock::new();
                let session = SESSION.get_or_init(|| {
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_nanos()
                });
                let snapshot = path
                    .parent()
                    .unwrap_or(Path::new("."))
                    .join("snapshots")
                    .join(format!(
                        "{}-{}-{session}-{:x}.gitconfig",
                        profile.id,
                        std::process::id(),
                        md5::compute(text.as_bytes())
                    ));
                let lease = crate::connections::ConfigLease::acquire(snapshot.clone());
                if !snapshot.is_file() {
                    write_file(&snapshot, &text)?;
                }
                Ok(Some(lease))
            })
    });
    match result {
        Ok(lease) => {
            context.git_config = lease.as_ref().map(|lease| lease.path.clone());
            context.lease = lease;
            *lock(&ERROR) = None;
        }
        Err(error) => {
            context.error = Some(error.clone());
            *lock(&ERROR) = Some(format!(
                "The profile's connection settings aren't in use: {error}"
            ));
        }
    }
    crate::connections::set(context);
}

/// Why the active profile's git settings aren't in use, if they aren't.
pub fn error() -> Option<String> {
    lock(&ERROR).clone()
}

/// Deletes the config file of a profile that was deleted.
pub fn forget(id: u64) {
    if let Some(path) = config_path(id) {
        let _ = std::fs::remove_file(path);
    }
}
