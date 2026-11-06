//! Where a start finds the running instance: `instance.json` in the app's data folder, with
//! the port it listens on and its token. Only the user can read it: made so on Unix (mode
//! 0600), and in the user's own `%APPDATA%` on Windows.
//!
//! A crashed instance leaves it behind; nothing trusts it for that. A start only reads it while
//! another process holds the lock (see [`super::lock`]), and a port that doesn't answer with
//! that token is no instance.

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const FILE: &str = "instance.json";

/// Longer than any file this writes: what is longer is not read.
const MAX_SIZE: u64 = 4096;

/// The running instance, as its file says.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Found {
    /// The protocol it speaks (see [`super::message::VERSION`]).
    pub v: u64,
    pub port: u16,
    pub token: String,
    /// Its process, for whoever looks at the file.
    pub pid: u32,
}

fn path(dir: &Path) -> PathBuf {
    dir.join(FILE)
}

/// Writes `found` as the running instance: whole or not at all (a start reading it never sees
/// half of it), readable only by the user.
pub fn write(dir: &Path, found: &Found) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let tmp = dir.join(format!("{FILE}.{}.tmp", std::process::id()));
    let _ = std::fs::remove_file(&tmp);
    let written = (|| {
        let mut file = create_private(&tmp)?;
        file.write_all(serde_json::to_string(found)?.as_bytes())?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&tmp, path(dir))
    })();
    if written.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    written
}

/// A new file only the user can read or write.
fn create_private(path: &Path) -> std::io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

/// The running instance, as its file says; `None` when there is no file or it isn't one this
/// writes.
pub fn read(dir: &Path) -> Option<Found> {
    let mut text = String::new();
    File::open(path(dir))
        .ok()?
        .take(MAX_SIZE)
        .read_to_string(&mut text)
        .ok()?;
    let found: Found = serde_json::from_str(&text).ok()?;
    let token_ok = found.token.len() == 32 && found.token.bytes().all(|b| b.is_ascii_hexdigit());
    (found.port != 0 && token_ok).then_some(found)
}

/// Removes the file if it still names the instance with `token` (a newer one may have written
/// its own since).
pub fn remove_if_ours(dir: &Path, token: &str) {
    if read(dir).is_some_and(|f| f.token == token) {
        let _ = std::fs::remove_file(path(dir));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(token: &str) -> Found {
        Found {
            v: 1,
            port: 4321,
            token: token.to_owned(),
            pid: 7,
        }
    }

    #[test]
    fn written_is_read_back_and_replaces_the_last_one() -> std::io::Result<()> {
        let dir = super::super::temp_dir("write");
        assert_eq!(read(&dir), None);
        let first = found(&"a".repeat(32));
        write(&dir, &first)?;
        assert_eq!(read(&dir), Some(first));
        let second = found(&"b".repeat(32));
        write(&dir, &second)?;
        assert_eq!(read(&dir), Some(second));
        // Nothing left beside it.
        let names: Vec<_> = std::fs::read_dir(&dir)?
            .flatten()
            .map(|e| e.file_name())
            .collect();
        assert_eq!(names, vec![std::ffi::OsString::from(FILE)]);
        std::fs::remove_dir_all(&dir)
    }

    #[cfg(unix)]
    #[test]
    fn only_the_user_can_read_it() -> std::io::Result<()> {
        use std::os::unix::fs::PermissionsExt;
        let dir = super::super::temp_dir("mode");
        write(&dir, &found(&"c".repeat(32)))?;
        let mode = std::fs::metadata(path(&dir))?.permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        std::fs::remove_dir_all(&dir)
    }

    #[test]
    fn a_file_this_did_not_write_is_not_read() -> std::io::Result<()> {
        let dir = super::super::temp_dir("bad");
        std::fs::create_dir_all(&dir)?;
        for text in [
            "",
            "not json",
            r#"{"v":1,"port":0,"token":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","pid":1}"#,
            r#"{"v":1,"port":5,"token":"short","pid":1}"#,
            r#"{"v":1,"port":5,"token":"zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz","pid":1}"#,
        ] {
            std::fs::write(path(&dir), text)?;
            assert_eq!(read(&dir), None, "{text}");
        }
        // Longer than any this writes: not read whole.
        let long = format!(
            r#"{{"v":1,"port":5,"token":"{}","pid":1,"x":"{}"}}"#,
            "a".repeat(32),
            "x".repeat(8192)
        );
        std::fs::write(path(&dir), long)?;
        assert_eq!(read(&dir), None);
        std::fs::remove_dir_all(&dir)
    }

    #[test]
    fn only_its_own_file_is_removed() -> std::io::Result<()> {
        let dir = super::super::temp_dir("remove");
        let newer = found(&"d".repeat(32));
        write(&dir, &newer)?;
        remove_if_ours(&dir, &"e".repeat(32));
        assert_eq!(read(&dir), Some(newer.clone()));
        remove_if_ours(&dir, &newer.token);
        assert_eq!(read(&dir), None);
        std::fs::remove_dir_all(&dir)
    }
}
