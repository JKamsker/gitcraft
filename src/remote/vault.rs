//! Sign-ins kept for good ("Remember connection" in the sign-in dialog): a password or a key's
//! passphrase, in the system's credential store, which keeps it encrypted for this user:
//! Windows' Credential Manager, the macOS Keychain, the Secret Service (GNOME Keyring, KWallet)
//! on Linux. The app lists their names in a file of its own (so they can all be forgotten),
//! never the secrets.
//!
//! Each is reached by blocking calls (the Secret Service is another process): only from the
//! threads answering ssh, never the window's.

use std::path::PathBuf;
use std::sync::Mutex;

use super::shell::lock;

/// What the system's store files them under.
const SERVICE: &str = "gitcraft";

/// Where the secrets are kept, and the names of those kept.
trait Store: Send + Sync {
    fn get(&self, account: &str) -> Option<String>;
    fn set(&self, account: &str, secret: &str) -> Result<(), String>;
    fn delete(&self, account: &str);
    fn names(&self) -> Vec<String>;
    fn set_names(&self, names: &[String]) -> Result<(), String>;
}

/// The name a secret asked by `prompt` for `host` is kept under: readable in the system's
/// credential manager ("SSH · build | me@build's password:").
fn account(host: &str, prompt: &str) -> String {
    format!("{host} | {}", prompt.trim())
}

/// Changes to the list of names, one at a time.
static NAMES: Mutex<()> = Mutex::new(());

/// Why nothing is kept: a portable copy of gitcraft set to leave the credential store alone.
const STORE_OFF: &str = "this portable copy of gitcraft doesn't use the system's credential \
                         store (Settings → General → Portable mode)";

/// The secret kept for `prompt` of `host`, if one is (and the credential store is used).
pub(super) fn recall(host: &str, prompt: &str) -> Option<String> {
    if !crate::portable::credential_store() {
        return None;
    }
    store().get(&account(host, prompt))
}

/// Keeps `secret` for `prompt` of `host`; why it couldn't be.
pub(super) fn keep(host: &str, prompt: &str, secret: &str) -> Result<(), String> {
    if !crate::portable::credential_store() {
        return Err(STORE_OFF.to_owned());
    }
    let account = account(host, prompt);
    let store = store();
    store.set(&account, secret)?;
    let _one = lock(&NAMES);
    let mut names = store.names();
    if !names.contains(&account) {
        names.push(account);
        store.set_names(&names)?;
    }
    Ok(())
}

/// Forgets what is kept for `prompt` of `host`: whether something was.
pub(super) fn forget(host: &str, prompt: &str) -> bool {
    let account = account(host, prompt);
    let store = store();
    let was = store.get(&account).is_some();
    store.delete(&account);
    let _one = lock(&NAMES);
    let mut names = store.names();
    let before = names.len();
    names.retain(|n| *n != account);
    if names.len() != before {
        let _ = store.set_names(&names);
    }
    was
}

/// Forgets every sign-in kept: how many there were.
pub(super) fn forget_all() -> usize {
    forget_all_in(store())
}

fn forget_all_in(store: &dyn Store) -> usize {
    let _one = lock(&NAMES);
    let names = store.names();
    for name in &names {
        store.delete(name);
    }
    let _ = store.set_names(&[]);
    names.len()
}

/// The system's credential store.
#[cfg(not(test))]
fn store() -> &'static dyn Store {
    static SYSTEM: System = System;
    &SYSTEM
}

/// Tests never reach the system's store: one in memory stands in.
#[cfg(test)]
fn store() -> &'static dyn Store {
    static MEMORY: std::sync::OnceLock<tests::Memory> = std::sync::OnceLock::new();
    MEMORY.get_or_init(Default::default)
}

#[cfg_attr(test, allow(dead_code))]
struct System;

#[cfg_attr(test, allow(dead_code))]
impl System {
    fn entry(account: &str) -> Result<keyring::Entry, String> {
        keyring::Entry::new(SERVICE, account).map_err(|e| e.to_string())
    }

    /// The file listing the names kept.
    fn names_file() -> Option<PathBuf> {
        crate::portable::data_dir().map(|d| d.join("saved-sign-ins.json"))
    }
}

impl Store for System {
    fn get(&self, account: &str) -> Option<String> {
        Self::entry(account).ok()?.get_password().ok()
    }

    fn set(&self, account: &str, secret: &str) -> Result<(), String> {
        Self::entry(account)?
            .set_password(secret)
            .map_err(|e| e.to_string())
    }

    fn delete(&self, account: &str) {
        if let Ok(entry) = Self::entry(account) {
            let _ = entry.delete_credential();
        }
    }

    fn names(&self) -> Vec<String> {
        Self::names_file()
            .and_then(|path| std::fs::read(path).ok())
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    fn set_names(&self, names: &[String]) -> Result<(), String> {
        let path = Self::names_file().ok_or("no folder for this app's files")?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_vec_pretty(names).map_err(|e| e.to_string())?;
        std::fs::write(path, json).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
pub(super) mod tests {
    use std::collections::HashMap;

    use super::*;

    #[derive(Default)]
    pub(in crate::remote) struct Memory {
        secrets: Mutex<HashMap<String, String>>,
        names: Mutex<Vec<String>>,
    }

    impl Store for Memory {
        fn get(&self, account: &str) -> Option<String> {
            lock(&self.secrets).get(account).cloned()
        }

        fn set(&self, account: &str, secret: &str) -> Result<(), String> {
            lock(&self.secrets).insert(account.to_owned(), secret.to_owned());
            Ok(())
        }

        fn delete(&self, account: &str) {
            lock(&self.secrets).remove(account);
        }

        fn names(&self) -> Vec<String> {
            lock(&self.names).clone()
        }

        fn set_names(&self, names: &[String]) -> Result<(), String> {
            *lock(&self.names) = names.to_vec();
            Ok(())
        }
    }

    #[test]
    fn kept_secrets_are_recalled_until_forgotten() {
        let (host, prompt) = ("SSH · vault-one", "me@vault-one's password: ");
        assert_eq!(recall(host, prompt), None);
        assert_eq!(keep(host, prompt, "kept"), Ok(()));
        assert_eq!(recall(host, prompt), Some("kept".to_owned()));
        assert!(store().names().contains(&account(host, prompt)));
        // Kept again: replaced, listed once.
        assert_eq!(keep(host, prompt, "newer"), Ok(()));
        assert_eq!(recall(host, prompt), Some("newer".to_owned()));
        let listed = store().names();
        assert_eq!(
            listed
                .iter()
                .filter(|n| **n == account(host, prompt))
                .count(),
            1
        );
        assert!(forget(host, prompt));
        assert_eq!(recall(host, prompt), None);
        assert!(!store().names().contains(&account(host, prompt)));
        assert!(!forget(host, prompt));
    }

    /// Everything listed goes, from the store and the list.
    #[test]
    fn forgetting_all_empties_the_store() {
        let memory = Memory::default();
        for (account, secret) in [("a | p:", "1"), ("b | p:", "2")] {
            assert_eq!(memory.set(account, secret), Ok(()));
        }
        assert_eq!(
            memory.set_names(&["a | p:".to_owned(), "b | p:".to_owned()]),
            Ok(())
        );
        assert_eq!(forget_all_in(&memory), 2);
        assert!(memory.get("a | p:").is_none() && memory.get("b | p:").is_none());
        assert!(memory.names().is_empty());
        assert_eq!(forget_all_in(&memory), 0);
    }

    /// The system's own store keeps, gives back and forgets a secret (run by hand:
    /// `cargo test -- --ignored the_system_store`; it writes to this user's credential store).
    #[test]
    #[ignore = "uses the system's credential store"]
    fn the_system_store_keeps_and_forgets() {
        let account = "gitcraft test | me@nowhere's password:";
        assert_eq!(System.set(account, "pässwörd with spaces"), Ok(()));
        assert_eq!(System.get(account), Some("pässwörd with spaces".to_owned()));
        System.delete(account);
        assert_eq!(System.get(account), None);
    }

    #[test]
    fn names_say_the_machine_and_the_question() {
        assert_eq!(
            account("SSH · build", "me@build's password: "),
            "SSH · build | me@build's password:"
        );
    }
}
