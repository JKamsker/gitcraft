//! Which configuration wins, and from which file: every value git sees for a setting, with its
//! scope and file (`git config --list --show-origin --show-scope`), in the order git reads
//! them, the last one winning. Only the settings of [`SETTINGS`] are ever read or shown, so
//! nothing credential-related (helpers, tokens, extra headers) is.

use std::path::Path;

use crate::git::run;

/// One setting gitcraft explains: its key as git writes it, the group it is listed under, what
/// it does, and the values it takes (empty for a free text).
pub struct Setting {
    pub key: &'static str,
    pub group: &'static str,
    pub about: &'static str,
    pub choices: &'static [&'static str],
}

const fn setting(
    key: &'static str,
    group: &'static str,
    about: &'static str,
    choices: &'static [&'static str],
) -> Setting {
    Setting {
        key,
        group,
        about,
        choices,
    }
}

const BOOL: &[&str] = &["true", "false"];

/// The settings shown, by group. Nothing that holds or names a credential.
pub const SETTINGS: &[Setting] = &[
    setting(
        "user.name",
        "Identity",
        "The name commits are made with.",
        &[],
    ),
    setting(
        "user.email",
        "Identity",
        "The email commits are made with.",
        &[],
    ),
    setting(
        "pull.rebase",
        "Pull and push",
        "Whether pull rebases your commits onto what came in instead of merging.",
        &["false", "true", "merges", "interactive"],
    ),
    setting(
        "pull.ff",
        "Pull and push",
        "Whether pull may fast-forward, must, or always makes a merge commit.",
        &["true", "only", "false"],
    ),
    setting(
        "push.default",
        "Pull and push",
        "Which branch a plain push sends to.",
        &["simple", "current", "upstream", "matching", "nothing"],
    ),
    setting(
        "push.autoSetupRemote",
        "Pull and push",
        "Whether the first push of a new branch sets its upstream by itself.",
        BOOL,
    ),
    setting(
        "remote.pushDefault",
        "Pull and push",
        "The remote branches push to when they don't name one.",
        &[],
    ),
    setting(
        "fetch.prune",
        "Pull and push",
        "Whether fetch deletes remote branches that are gone on the remote.",
        BOOL,
    ),
    setting(
        "rebase.autoStash",
        "Pull and push",
        "Whether a rebase (and pull with rebase) stashes uncommitted changes first.",
        BOOL,
    ),
    setting(
        "merge.ff",
        "Pull and push",
        "Whether merge fast-forwards when it can.",
        &["true", "only", "false"],
    ),
    setting(
        "commit.gpgSign",
        "Signing",
        "Whether every commit is signed.",
        BOOL,
    ),
    setting(
        "tag.gpgSign",
        "Signing",
        "Whether every tag is signed.",
        BOOL,
    ),
    setting(
        "gpg.format",
        "Signing",
        "What signs: OpenPGP, SSH keys or X.509 certificates.",
        &["openpgp", "ssh", "x509"],
    ),
    setting(
        "user.signingKey",
        "Signing",
        "The key (or the SSH public key's file) commits and tags are signed with.",
        &[],
    ),
    setting(
        "gpg.program",
        "Signing",
        "The program OpenPGP signs with.",
        &[],
    ),
    setting(
        "gpg.ssh.program",
        "Signing",
        "The program SSH signing runs.",
        &[],
    ),
    setting(
        "core.autocrlf",
        "Line endings",
        "Whether git turns CRLF into LF when it stores files, and back when it writes them out.",
        &["true", "input", "false"],
    ),
    setting(
        "core.eol",
        "Line endings",
        "The line ending text files get in the working tree, when .gitattributes doesn't say.",
        &["native", "lf", "crlf"],
    ),
    setting(
        "core.safecrlf",
        "Line endings",
        "Whether git warns, or refuses, when converting line endings would change a file.",
        &["true", "warn", "false"],
    ),
    setting(
        "core.ignoreCase",
        "Files",
        "Whether file names that differ only in case are the same file.",
        BOOL,
    ),
    setting(
        "core.fileMode",
        "Files",
        "Whether a change of a file's executable bit counts as a change.",
        BOOL,
    ),
    setting(
        "core.symlinks",
        "Files",
        "Whether symbolic links are checked out as links or as small text files.",
        BOOL,
    ),
    setting(
        "core.longpaths",
        "Files",
        "Windows: whether paths longer than 260 characters work.",
        BOOL,
    ),
    setting(
        "core.excludesFile",
        "Files",
        "Your own ignore file, besides each repository's .gitignore.",
        &[],
    ),
    setting(
        "core.attributesFile",
        "Files",
        "Your own attributes file, besides each repository's .gitattributes.",
        &[],
    ),
    setting(
        "core.hooksPath",
        "Files",
        "The folder git runs hooks from instead of .git/hooks.",
        &[],
    ),
    setting(
        "core.fsmonitor",
        "Performance",
        "Whether a file system monitor tells git what changed, so status needn't look at every file.",
        BOOL,
    ),
    setting(
        "core.untrackedCache",
        "Performance",
        "Whether git remembers which folders have untracked files.",
        BOOL,
    ),
    setting(
        "core.commitGraph",
        "Performance",
        "Whether git reads the commit-graph file, which makes history faster to walk.",
        BOOL,
    ),
    setting(
        "fetch.writeCommitGraph",
        "Performance",
        "Whether every fetch updates the commit-graph.",
        BOOL,
    ),
    setting(
        "feature.manyFiles",
        "Performance",
        "Turns on the settings that help repositories with many files.",
        BOOL,
    ),
    setting(
        "init.defaultBranch",
        "Other",
        "The name of the first branch of a new repository.",
        &[],
    ),
    setting(
        "merge.conflictStyle",
        "Other",
        "How conflicts are written into files: with the common ancestor or without.",
        &["merge", "diff3", "zdiff3"],
    ),
    setting(
        "rerere.enabled",
        "Other",
        "Whether git remembers how you resolved a conflict and resolves it so again.",
        BOOL,
    ),
    setting(
        "core.editor",
        "Other",
        "The editor git opens for messages.",
        &[],
    ),
];

/// The setting of [`SETTINGS`] with this key (git's keys are case-insensitive outside the
/// subsection, and these have none).
pub fn find(key: &str) -> Option<&'static Setting> {
    SETTINGS.iter().find(|s| s.key.eq_ignore_ascii_case(key))
}

/// Where a value comes from, from the least to the most important (git reads them in this
/// order, the later one winning).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Scope {
    System,
    Global,
    Local,
    Worktree,
    /// `-c` or `GIT_CONFIG_PARAMETERS`: only for the commands run so.
    Command,
}

impl Scope {
    fn parse(word: &str) -> Option<Self> {
        Some(match word {
            "system" => Scope::System,
            "global" => Scope::Global,
            "local" => Scope::Local,
            "worktree" => Scope::Worktree,
            "command" => Scope::Command,
            _ => return None,
        })
    }

    pub fn label(self) -> &'static str {
        match self {
            Scope::System => "System",
            Scope::Global => "Global",
            Scope::Local => "Repository",
            Scope::Worktree => "Worktree",
            Scope::Command => "Command line",
        }
    }

    /// The `git config` option that writes at this scope, for those gitcraft writes to.
    pub fn option(self) -> Option<&'static str> {
        match self {
            Scope::System => Some("--system"),
            Scope::Global => Some("--global"),
            Scope::Local => Some("--local"),
            Scope::Worktree => Some("--worktree"),
            Scope::Command => None,
        }
    }

    pub const WRITABLE: [Scope; 4] = [Scope::Local, Scope::Worktree, Scope::Global, Scope::System];
}

/// One value git read for a setting.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Origin {
    pub scope: Scope,
    /// The file it is in (as git names it: `.git/config`, `/home/me/.gitconfig`), or what
    /// else gave it (`command line:`).
    pub file: Option<String>,
    /// The key as git prints it (lowercase).
    pub key: String,
    /// `None` for a key with no `=` (true, for a boolean).
    pub value: Option<String>,
}

impl Origin {
    /// The value as it reads: a bare key is a boolean set to true.
    pub fn shown_value(&self) -> &str {
        self.value.as_deref().unwrap_or("true")
    }
}

/// Every value git sees for the settings of [`SETTINGS`], in the order it reads them.
pub fn origins(repo: &Path) -> Result<Vec<Origin>, String> {
    let out = run(
        Some(repo),
        &[
            "config",
            "--list",
            "--show-origin",
            "--show-scope",
            "--includes",
            "-z",
        ],
    )
    .map_err(|e| format!("Could not read the configuration: {e}"))?;
    Ok(parse_origins(&out)
        .into_iter()
        .filter(|o| find(&o.key).is_some())
        .collect())
}

/// Reads `git config --list --show-origin --show-scope -z`: per value `scope NUL origin NUL
/// key LF value NUL` (no LF and no value for a bare key). Entries of an unknown scope (a newer
/// git's) are skipped.
pub fn parse_origins(out: &str) -> Vec<Origin> {
    let mut fields = out.split('\0');
    let mut list = Vec::new();
    while let (Some(scope), Some(origin), Some(entry)) =
        (fields.next(), fields.next(), fields.next())
    {
        let Some(scope) = Scope::parse(scope) else {
            continue;
        };
        let (key, value) = match entry.split_once('\n') {
            Some((key, value)) => (key, Some(value.to_owned())),
            None => (entry, None),
        };
        let file = origin
            .strip_prefix("file:")
            .map(str::to_owned)
            .or_else(|| (!origin.is_empty()).then(|| origin.to_owned()));
        list.push(Origin {
            scope,
            file,
            key: key.to_owned(),
            value,
        });
    }
    list
}

/// The values of `key` among `all`, in git's order; the last one is the one in effect.
pub fn values_of<'a>(all: &'a [Origin], key: &str) -> Vec<&'a Origin> {
    all.iter()
        .filter(|o| o.key.eq_ignore_ascii_case(key))
        .collect()
}

/// The `git config` arguments that set `key` to `value` at `scope`.
pub fn set_args(scope: Scope, key: &str, value: &str) -> Result<Vec<String>, String> {
    let option = scope
        .option()
        .ok_or_else(|| "A command-line value can't be written".to_owned())?;
    if find(key).is_none() {
        return Err(format!("{key} is not a setting gitcraft changes"));
    }
    if value.contains(['\n', '\r', '\0']) {
        return Err("A value can't hold a line break".to_owned());
    }
    Ok(["config", option, "--", key, value]
        .map(str::to_owned)
        .to_vec())
}

/// The `git config` arguments that remove every value of `key` from `file` (as git named it),
/// so that what the other files say applies again.
pub fn unset_args(origin: &Origin) -> Result<Vec<String>, String> {
    let file = match (&origin.scope, &origin.file) {
        (Scope::Command, _) | (_, None) => {
            return Err("A value given on the command line can't be removed here".to_owned());
        }
        (_, Some(file)) => file,
    };
    Ok(["config", "--file", file, "--unset-all", "--", &origin.key]
        .map(|s| s.to_owned())
        .to_vec())
}

/// What `key` is in each of `worktrees` (folders of the repository's worktrees): the value in
/// effect there and its scope, `None` when it isn't set there.
pub fn in_worktrees(
    worktrees: &[std::path::PathBuf],
    key: &str,
) -> Vec<(std::path::PathBuf, Result<Option<Origin>, String>)> {
    worktrees
        .iter()
        .map(|dir| {
            let found = origins(dir).map(|all| values_of(&all, key).last().map(|o| (*o).clone()));
            (dir.clone(), found)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_scopes_files_and_bare_keys() {
        let out = "system\0file:/etc/gitconfig\0pull.rebase\nfalse\0\
                   global\0file:/home/me/.gitconfig\0pull.rebase\ntrue\0\
                   local\0file:.git/config\0core.fsmonitor\0\
                   command\0command line:\0core.editor\nvim\0\
                   future\0file:x\0a.b\nc\0\
                   local\0file:.git/config\0user.name\nZoë Ünïcode\nsecond line\0";
        let all = parse_origins(out);
        assert_eq!(all.len(), 5);
        assert_eq!(all[0].scope, Scope::System);
        assert_eq!(all[0].file.as_deref(), Some("/etc/gitconfig"));
        assert_eq!(all[1].value.as_deref(), Some("true"));
        assert_eq!(all[2].value, None);
        assert_eq!(all[2].shown_value(), "true");
        assert_eq!(all[3].file.as_deref(), Some("command line:"));
        // A value may itself hold a line break: only the first one ends the key.
        assert_eq!(all[4].value.as_deref(), Some("Zoë Ünïcode\nsecond line"));
        let rebase = values_of(&all, "pull.Rebase");
        assert_eq!(rebase.len(), 2);
        assert_eq!(rebase.last().map(|o| o.scope), Some(Scope::Global));
    }

    #[test]
    fn empty_output_is_no_values() {
        assert!(parse_origins("").is_empty());
        assert!(parse_origins("local\0").is_empty());
    }

    #[test]
    fn nothing_credential_related_is_listed() {
        for s in SETTINGS {
            let key = s.key.to_lowercase();
            for word in [
                "credential",
                "token",
                "password",
                "secret",
                "extraheader",
                "auth",
            ] {
                assert!(!key.contains(word), "{key}");
            }
        }
        assert!(find("credential.helper").is_none());
        assert!(find("PULL.REBASE").is_some());
    }

    #[test]
    fn writes_are_checked() {
        assert_eq!(
            set_args(Scope::Local, "pull.rebase", "true"),
            Ok(["config", "--local", "--", "pull.rebase", "true"]
                .map(str::to_owned)
                .to_vec())
        );
        assert!(set_args(Scope::Command, "pull.rebase", "true").is_err());
        assert!(set_args(Scope::Local, "credential.helper", "store").is_err());
        assert!(set_args(Scope::Local, "user.name", "a\nb").is_err());
        let origin = Origin {
            scope: Scope::Global,
            file: Some("/home/me/.gitconfig".into()),
            key: "pull.rebase".into(),
            value: Some("true".into()),
        };
        assert_eq!(
            unset_args(&origin).map(|a| a[2].clone()),
            Ok("/home/me/.gitconfig".to_owned())
        );
        let command = Origin {
            scope: Scope::Command,
            ..origin
        };
        assert!(unset_args(&command).is_err());
    }
}
