//! Portable Git preferences are values, never arbitrary config, paths or executable hooks.
use super::{reference, text};
use crate::git;
use std::{collections::BTreeMap, path::Path};
pub(super) type Values = BTreeMap<String, String>;
pub(super) fn allowed(key: &str, value: &str) -> bool {
    let v = value.to_ascii_lowercase();
    match key {
        "core.autocrlf" => ["true", "false", "input"].contains(&v.as_str()),
        "core.eol" => ["lf", "crlf", "native"].contains(&v.as_str()),
        "core.filemode"
        | "core.ignorecase"
        | "core.symlinks"
        | "fetch.prune"
        | "fetch.prunetags"
        | "rebase.autostash"
        | "rebase.updaterefs"
        | "push.autosetupremote" => ["true", "false"].contains(&v.as_str()),
        "pull.ff" | "merge.ff" => ["true", "false", "only"].contains(&v.as_str()),
        "pull.rebase" => ["true", "false", "merges", "interactive"].contains(&v.as_str()),
        "push.default" => {
            ["simple", "current", "upstream", "matching", "nothing"].contains(&v.as_str())
        }
        "status.showuntrackedfiles" => ["no", "normal", "all"].contains(&v.as_str()),
        "diff.algorithm" => ["myers", "minimal", "patience", "histogram"].contains(&v.as_str()),
        _ => {
            let Some(branch) = key.strip_prefix("branch.") else {
                return false;
            };
            let Some((branch, field)) = branch.rsplit_once('.') else {
                return false;
            };
            if reference(branch).is_err() {
                return false;
            }
            match field {
                "remote" | "pushremote" => value == "." || reference(value).is_ok(),
                "merge" => value.starts_with("refs/heads/") && super::full_ref(value).is_ok(),
                "rebase" => ["true", "false", "merges", "interactive"].contains(&v.as_str()),
                _ => false,
            }
        }
    }
}
pub(super) fn validate(values: &Values) -> Result<(), String> {
    if values.len() > 2048 {
        return Err("Too many portable configuration values".into());
    }
    for (key, value) in values {
        text(key, 512)?;
        text(value, 1024)?;
        if !allowed(key, value) {
            return Err(format!(
                "Unsupported portable configuration key/value: {key}"
            ));
        }
    }
    Ok(())
}
pub(super) fn snapshot(repo: &Path, active: &dyn Fn() -> bool) -> Result<Values, String> {
    if !active() {
        return Err("Export cancelled".into());
    }
    let raw = git::connection_read(
        repo,
        &[
            "config",
            "--local",
            "--no-includes",
            "--null",
            "--get-regexp",
            "^(core\\.(autocrlf|eol|filemode|ignorecase|symlinks)|fetch\\.(prune|prunetags)|pull\\.(ff|rebase)|merge\\.ff|rebase\\.(autostash|updaterefs)|push\\.(default|autosetupremote)|status\\.showuntrackedfiles|diff\\.algorithm|branch\\..*\\.(remote|pushremote|merge|rebase))$",
        ],
    )?;
    let mut out = Values::new();
    for field in raw.split('\0').filter(|s| !s.is_empty()) {
        let (key, value) = field
            .split_once('\n')
            .ok_or("Invalid portable configuration response")?;
        if allowed(key, value) {
            out.insert(key.into(), value.into());
        }
    }
    validate(&out)?;
    Ok(out)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn commands_paths_and_arbitrary_config_are_refused() {
        assert!(allowed("pull.rebase", "merges"));
        assert!(allowed("branch.feature/x.merge", "refs/heads/feature/x"));
        for (key, value) in [
            ("core.sshcommand", "steal"),
            ("core.hookspath", "/tmp/hooks"),
            ("include.path", "/tmp/config"),
            ("http.extraheader", "secret"),
            ("pull.rebase", "!command"),
        ] {
            assert!(!allowed(key, value));
        }
    }
}
