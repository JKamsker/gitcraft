//! The config changes that fix a known failure (see [`super::GitProblem`]), each one undone
//! by [`revert`]. They run through the command layer, so on the repository's machine.

use std::path::Path;

use crate::git::{run, run_ok_codes};

/// Which config file an edit goes to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Scope {
    /// The user's own global config (`git config --global`: never a profile's file, see
    /// `command.rs`).
    Global,
    /// The repository's own config (`.git/config`).
    Repo,
    /// A config file named (`git config --file`), for tests.
    #[cfg(test)]
    File(std::path::PathBuf),
}

impl Scope {
    fn args(&self) -> Vec<String> {
        match self {
            Scope::Global => vec!["--global".to_owned()],
            Scope::Repo => vec!["--local".to_owned()],
            #[cfg(test)]
            Scope::File(path) => vec!["--file".to_owned(), path.display().to_string()],
        }
    }
}

/// One change to a config file, and what it replaced.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfigEdit {
    pub scope: Scope,
    pub key: &'static str,
    pub value: String,
    /// Added beside the values already there (`safe.directory`), rather than replacing one.
    pub add: bool,
    /// The value it replaced, put back by [`revert`]; `None` when there was none.
    pub previous: Option<String>,
}

impl ConfigEdit {
    /// Sets `key` to `value` in `scope`, keeping what it was before (read in `dir`) for undoing.
    pub fn set(dir: &Path, scope: Scope, key: &'static str, value: &str) -> Result<Self, String> {
        let previous = get(dir, &scope, key)?;
        Ok(Self {
            scope,
            key,
            value: value.to_owned(),
            add: false,
            previous,
        })
    }

    /// Adds `value` to the values of `key` in `scope`.
    pub fn add(scope: Scope, key: &'static str, value: &str) -> Self {
        Self {
            scope,
            key,
            value: value.to_owned(),
            add: true,
            previous: None,
        }
    }

    /// `key = value`, as the confirmation shows it.
    pub fn line(&self) -> String {
        format!("{} = {}", self.key, self.value)
    }
}

/// `git config <scope> args…` in `dir`.
fn config(dir: &Path, scope: &Scope, args: &[&str], ok: &[i32]) -> Result<String, String> {
    let scope = scope.args();
    let mut all = vec!["config"];
    all.extend(scope.iter().map(String::as_str));
    all.extend_from_slice(args);
    run_ok_codes(Some(dir), &all, ok)
}

/// The value of `key` in `scope` (the last one, for a key set more than once), if set.
pub fn get(dir: &Path, scope: &Scope, key: &str) -> Result<Option<String>, String> {
    // 1: not set.
    let out = config(dir, scope, &["--get", key], &[0, 1])?;
    let value = out.strip_suffix('\n').unwrap_or(&out);
    Ok((!out.is_empty()).then(|| value.to_owned()))
}

/// Every value of `key` in `scope`.
pub fn get_all(dir: &Path, scope: &Scope, key: &str) -> Result<Vec<String>, String> {
    let out = config(dir, scope, &["--null", "--get-all", key], &[0, 1])?;
    Ok(out
        .split('\0')
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
        .collect())
}

/// The file a change to `scope` is written to, on the machine `dir` is on, as git names it.
/// For the global scope git reads `~/.gitconfig` and `$XDG_CONFIG_HOME/git/config` but writes
/// the first when it is there: of the files it names, that one wins.
pub fn file_of(dir: &Path, scope: &Scope) -> Result<String, String> {
    let out = match config(dir, scope, &["--show-origin", "--null", "--list"], &[0, 1]) {
        Ok(out) => out,
        // No such file yet (git exits 128): git makes it on the first write.
        Err(e) if e.contains("unable to read config file") => String::new(),
        Err(e) => return Err(e),
    };
    let origins = origins(&out);
    let chosen = origins
        .iter()
        .find(|path| path.ends_with("/.gitconfig") || path.ends_with("\\.gitconfig"))
        .or_else(|| origins.first());
    match (chosen, scope) {
        (Some(path), _) => Ok(path.clone()),
        // Nothing in it yet: git writes ~/.gitconfig.
        (None, Scope::Global) => Ok("~/.gitconfig".to_owned()),
        (None, Scope::Repo) => {
            let dir_out = run(
                Some(dir),
                &["rev-parse", "--path-format=absolute", "--git-common-dir"],
            )?;
            Ok(format!("{}/config", dir_out.trim_end_matches(['\n', '\r'])))
        }
        #[cfg(test)]
        (None, Scope::File(path)) => Ok(path.display().to_string()),
    }
}

/// The files `git config --show-origin --null --list` names, in order, each once:
/// `file:<path>\0key\nvalue\0…`.
pub(super) fn origins(out: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for (i, field) in out.split('\0').enumerate() {
        if i % 2 != 0 {
            continue;
        }
        if let Some(path) = field.strip_prefix("file:").filter(|p| !p.is_empty())
            && !found.iter().any(|f| f == path)
        {
            found.push(path.to_owned());
        }
    }
    found
}

/// Makes the change, in `dir`.
pub fn apply(dir: &Path, edit: &ConfigEdit) -> Result<(), String> {
    let value = edit.value.as_str();
    let args: &[&str] = if edit.add {
        &["--add", edit.key, value]
    } else {
        &[edit.key, value]
    };
    config(dir, &edit.scope, args, &[0]).map(|_| ())
}

/// Takes the change back: the value added comes out (that value only), the value replaced
/// comes back, or the key is unset again.
pub fn revert(dir: &Path, edit: &ConfigEdit) -> Result<(), String> {
    // 5: it wasn't there (any more).
    let unset = |args: &[&str]| config(dir, &edit.scope, args, &[0, 5]).map(|_| ());
    match (&edit.previous, edit.add) {
        (_, true) => unset(&["--unset-all", edit.key, &exact_pattern(&edit.value)]),
        (Some(previous), false) => {
            config(dir, &edit.scope, &[edit.key, previous], &[0]).map(|_| ())
        }
        (None, false) => unset(&["--unset", edit.key]),
    }
}

/// A value pattern (an extended regular expression, as `git config` takes one) matching
/// exactly `value`.
pub(super) fn exact_pattern(value: &str) -> String {
    let mut pattern = String::with_capacity(value.len() + 2);
    pattern.push('^');
    for c in value.chars() {
        if r"\.^$*+?()[]{}|".contains(c) {
            pattern.push('\\');
        }
        pattern.push(c);
    }
    pattern.push('$');
    pattern
}
