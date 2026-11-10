//! Profile connection choices. Never switch a CLI's global active account or store a token.

use std::cell::RefCell;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
#[cfg(all(test, unix))]
use std::process::Stdio;
use std::sync::{Arc, Mutex, RwLock, Weak};
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Account {
    pub host: String,
    pub user: String,
    /// Optional independent CLI configuration folder, e.g. two glab accounts on one host.
    pub config_dir: String,
}

impl Account {
    pub fn on(host: &str) -> Self {
        Self {
            host: host.to_owned(),
            ..Self::default()
        }
    }
    pub fn selected(&self) -> bool {
        !self.user.trim().is_empty()
    }
    pub fn validate(&self) -> Result<(), String> {
        if !self.selected() {
            return if self.config_dir.trim().is_empty() {
                Ok(())
            } else {
                Err("Choose the account's username as well as its configuration folder".to_owned())
            };
        }
        if self.host.is_empty()
            || self.host.starts_with(['-', '.'])
            || !self
                .host
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b':'))
        {
            return Err("An account needs a plain server hostname (and optional port)".to_owned());
        }
        if self.user.starts_with('-')
            || !self
                .user
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
        {
            return Err("Choose a plain account username".to_owned());
        }
        if !self.config_dir.trim().is_empty() && !Path::new(&self.config_dir).is_absolute() {
            return Err("The account configuration folder must be an absolute path".to_owned());
        }
        if self.config_dir.chars().any(char::is_control) {
            return Err("The account folder cannot contain control characters".to_owned());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Choices {
    pub azure: crate::forge::cloud::Account,
    pub bitbucket: crate::forge::cloud::Account,
    /// Selected tracker accounts and secure-store references only; no exported tokens.
    pub jira: crate::trackers::Account,
    pub linear: crate::trackers::Account,
    pub ssh_key: String,
    pub github: Account,
    pub gitlab: Account,
    pub tea_login: String,
    /// HTTPS host for the selected tea alias; metadata only, never its token.
    pub tea_host: String,
}

impl Default for Choices {
    fn default() -> Self {
        Self {
            azure: Default::default(),
            bitbucket: Default::default(),
            jira: Default::default(),
            linear: Default::default(),
            ssh_key: String::new(),
            github: Account::on("github.com"),
            gitlab: Account::on("gitlab.com"),
            tea_login: String::new(),
            tea_host: String::new(),
        }
    }
}

impl Choices {
    pub fn validate(&self) -> Result<(), String> {
        self.azure.validate(crate::forge::cloud::Service::Azure)?;
        self.bitbucket
            .validate(crate::forge::cloud::Service::Bitbucket)?;
        self.jira.validate(crate::trackers::Kind::Jira)?;
        self.linear.validate(crate::trackers::Kind::Linear)?;
        self.github.validate()?;
        self.gitlab.validate()?;
        if !self.tea_login.is_empty() {
            Account {
                host: self.tea_host.clone(),
                user: "tea".to_owned(),
                config_dir: String::new(),
            }
            .validate()
            .map_err(|_| {
                "Set the tea login's HTTPS server, or use Check account to find it".to_owned()
            })?;
        }
        if self.ssh_key.chars().any(char::is_control)
            || self.tea_login.chars().any(char::is_control)
        {
            return Err(
                "SSH key paths and tea login names cannot contain control characters".to_owned(),
            );
        }
        if !self.ssh_key.is_empty() && !Path::new(&self.ssh_key).is_absolute() {
            return Err("Choose an absolute path to the SSH private key".to_owned());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct Context {
    pub git_config: Option<PathBuf>,
    pub lease: Option<Arc<ConfigLease>>,
    pub choices: Choices,
    pub error: Option<String>,
}

/// A worker holds its immutable config alive across profile edits/deletion/id reuse.
#[derive(Debug, PartialEq, Eq, Hash)]
pub(crate) struct ConfigLease {
    pub path: PathBuf,
}
fn leases() -> &'static Mutex<std::collections::HashMap<PathBuf, Weak<ConfigLease>>> {
    static LEASES: std::sync::OnceLock<
        Mutex<std::collections::HashMap<PathBuf, Weak<ConfigLease>>>,
    > = std::sync::OnceLock::new();
    LEASES.get_or_init(Default::default)
}
impl ConfigLease {
    pub fn acquire(path: PathBuf) -> Arc<Self> {
        let mut known = leases()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(lease) = known.get(&path).and_then(Weak::upgrade) {
            return lease;
        }
        let lease = Arc::new(Self { path: path.clone() });
        known.insert(path, Arc::downgrade(&lease));
        lease
    }
}
impl Drop for ConfigLease {
    fn drop(&mut self) {
        let mut known = leases()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if known
            .get(&self.path)
            .is_some_and(|weak| std::ptr::eq(weak.as_ptr(), self))
        {
            known.remove(&self.path);
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

static ACTIVE: RwLock<Option<Context>> = RwLock::new(None);
static GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Observe every actual transition, including A→B→A between UI callbacks.
/// Saturation is treated as permanently changed by consumers.
pub(crate) fn generation() -> u64 {
    GENERATION.load(std::sync::atomic::Ordering::Acquire)
}
thread_local! { static PINNED: RefCell<Option<Context>> = const { RefCell::new(None) }; }

pub(crate) fn set(context: Context) {
    let mut active = ACTIVE
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if active.as_ref() != Some(&context) {
        *active = Some(context);
        let _ = GENERATION.fetch_update(
            std::sync::atomic::Ordering::AcqRel,
            std::sync::atomic::Ordering::Acquire,
            |generation| Some(generation.saturating_add(1)),
        );
    }
}

pub(crate) fn current() -> Context {
    PINNED
        .with(|value| value.borrow().clone())
        .unwrap_or_else(|| {
            ACTIVE
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone()
                .unwrap_or_default()
        })
}

impl Context {
    /// Nested jobs keep the initiating profile even if the UI switches meanwhile.
    pub fn run<T>(&self, job: impl FnOnce() -> T) -> T {
        struct Restore(Option<Context>);
        impl Drop for Restore {
            fn drop(&mut self) {
                PINNED.with(|p| *p.borrow_mut() = self.0.take());
            }
        }
        let _restore = Restore(PINNED.with(|p| p.replace(Some(self.clone()))));
        job()
    }
}

/// Capture both the profile and task lifetime before entering any nested thread.
pub(crate) fn inherit<F: FnOnce() -> T, T>(job: F) -> impl FnOnce() -> T {
    let context = current();
    let cancel = crate::tasks::cancellation();
    move || crate::tasks::with_cancellation(cancel, || context.run(job))
}
pub(crate) fn spawn<T: Send + 'static>(
    job: impl FnOnce() -> T + Send + 'static,
) -> std::thread::JoinHandle<T> {
    std::thread::spawn(inherit(job))
}

/// Git executes core.sshCommand with its shell, including on Git for Windows.
pub(crate) fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

pub(crate) fn sign_in_command(tool: &str, account: &Account) -> String {
    let variable = if tool == "gh" {
        "GH_CONFIG_DIR"
    } else {
        "GLAB_CONFIG_DIR"
    };
    #[cfg(windows)]
    {
        let quote = |s: &str| format!("'{}'", s.replace('\'', "''"));
        format!(
            "$env:{variable}={}; {tool} auth login --hostname {}",
            quote(&account.config_dir),
            quote(&account.host)
        )
    }
    #[cfg(not(windows))]
    {
        format!(
            "{variable}={} {tool} auth login --hostname {}",
            shell_quote(&account.config_dir),
            shell_quote(&account.host)
        )
    }
}

/// Metadata only: the token is resolved by this process when Git actually asks for a secret.
pub(crate) fn helper_command(tool: &str, account: &Account, executable: &Path) -> String {
    #[cfg(windows)]
    let executable = PathBuf::from(executable.to_string_lossy().replace('\\', "/"));
    let encoded = URL_SAFE_NO_PAD.encode(serde_json::to_vec(account).unwrap_or_default());
    format!(
        "!{} --gitcraft-profile-credential {tool} {encoded}",
        shell_quote(&executable.to_string_lossy())
    )
}

#[cfg(test)]
thread_local! { pub(crate) static TEST_PROGRAMS: RefCell<std::collections::HashMap<String,PathBuf>> = RefCell::new(std::collections::HashMap::new()); }

mod credentials;
pub use credentials::*;

#[cfg(test)]
mod tests;

mod ssh;
pub use ssh::*;

/// Nonsecret metadata for push-preflight diagnostics. A transport username such as SSH's
/// `git` is not this account, and a tea login alias is not necessarily its user's name.
#[allow(dead_code)] // Consumed by the independently developed push-preflight feature at integration.
pub(crate) fn expected_account(host: &str) -> Option<String> {
    let context = current();
    [&context.choices.github, &context.choices.gitlab]
        .into_iter()
        .find(|account| account.selected() && account.host.eq_ignore_ascii_case(host))
        .map(|account| account.user.clone())
}
