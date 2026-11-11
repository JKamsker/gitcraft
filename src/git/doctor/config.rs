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

impl Setting {
    pub fn group_label(&self) -> &'static str {
        crate::i18n::text(self.group)
    }
    pub fn about_label(&self) -> &'static str {
        crate::i18n::text(self.about)
    }
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
        "backend-config-group-identity",
        "backend-the-name-commits-are-made-with-3a8433",
        &[],
    ),
    setting(
        "user.email",
        "backend-config-group-identity",
        "backend-the-email-commits-are-made-with-293b54",
        &[],
    ),
    setting(
        "pull.rebase",
        "backend-pull-and-push-b471d5",
        "backend-whether-pull-rebases-your-commits-onto-what-came-in-instead-of-me-69836e",
        &["false", "true", "merges", "interactive"],
    ),
    setting(
        "pull.ff",
        "backend-pull-and-push-b471d5",
        "backend-whether-pull-may-fast-forward-must-or-always-makes-a-merge-commit-e6f0d7",
        &["true", "only", "false"],
    ),
    setting(
        "push.default",
        "backend-pull-and-push-b471d5",
        "backend-which-branch-a-plain-push-sends-to-ace7b5",
        &["simple", "current", "upstream", "matching", "nothing"],
    ),
    setting(
        "push.autoSetupRemote",
        "backend-pull-and-push-b471d5",
        "backend-whether-the-first-push-of-a-new-branch-sets-its-upstream-by-itsel-54b953",
        BOOL,
    ),
    setting(
        "remote.pushDefault",
        "backend-pull-and-push-b471d5",
        "backend-the-remote-branches-push-to-when-they-don-t-name-one-1a7c88",
        &[],
    ),
    setting(
        "fetch.prune",
        "backend-pull-and-push-b471d5",
        "backend-whether-fetch-deletes-your-remote-tracking-branches-origin-whose-87d48b",
        BOOL,
    ),
    setting(
        "rebase.autoStash",
        "backend-pull-and-push-b471d5",
        "backend-whether-a-rebase-and-pull-with-rebase-stashes-uncommitted-changes-d35cc3",
        BOOL,
    ),
    setting(
        "merge.ff",
        "backend-pull-and-push-b471d5",
        "backend-whether-merge-fast-forwards-when-it-can-138b7b",
        &["true", "only", "false"],
    ),
    setting(
        "commit.gpgSign",
        "backend-config-group-signing",
        "backend-whether-every-commit-is-signed-b6ec3b",
        BOOL,
    ),
    setting(
        "tag.gpgSign",
        "backend-config-group-signing",
        "backend-whether-every-tag-is-signed-e87c27",
        BOOL,
    ),
    setting(
        "gpg.format",
        "backend-config-group-signing",
        "backend-what-commits-and-tags-are-signed-with-openpgp-ssh-keys-or-x-509-c-d2a23b",
        &["openpgp", "ssh", "x509"],
    ),
    setting(
        "user.signingKey",
        "backend-config-group-signing",
        "backend-the-key-or-the-ssh-public-key-s-file-commits-and-tags-are-signed-206e8d",
        &[],
    ),
    setting(
        "gpg.program",
        "backend-config-group-signing",
        "backend-the-program-openpgp-signs-with-257c3c",
        &[],
    ),
    setting(
        "gpg.ssh.program",
        "backend-config-group-signing",
        "backend-the-program-ssh-signing-runs-a3a763",
        &[],
    ),
    setting(
        "core.autocrlf",
        "backend-line-endings-c948fb",
        "backend-true-git-turns-crlf-into-lf-when-it-stores-files-and-back-when-it-b348f3",
        &["true", "input", "false"],
    ),
    setting(
        "core.eol",
        "backend-line-endings-c948fb",
        "backend-the-line-ending-text-files-get-in-the-working-tree-when-gitattrib-5e02b9",
        &["native", "lf", "crlf"],
    ),
    setting(
        "core.safecrlf",
        "backend-line-endings-c948fb",
        "backend-whether-git-warns-or-refuses-when-converting-line-endings-couldn-32c843",
        &["true", "warn", "false"],
    ),
    setting(
        "core.ignoreCase",
        "backend-config-group-files",
        "backend-whether-file-names-that-differ-only-in-case-are-the-same-file-git-5f8f3f",
        BOOL,
    ),
    setting(
        "core.fileMode",
        "backend-config-group-files",
        "backend-whether-a-change-of-a-file-s-executable-bit-counts-as-a-change-gi-000286",
        BOOL,
    ),
    setting(
        "core.symlinks",
        "backend-config-group-files",
        "backend-whether-symbolic-links-are-checked-out-as-links-or-as-small-text-ea1851",
        BOOL,
    ),
    setting(
        "core.longpaths",
        "backend-config-group-files",
        "backend-windows-whether-paths-longer-than-260-characters-work-b2abe7",
        BOOL,
    ),
    setting(
        "core.excludesFile",
        "backend-config-group-files",
        "backend-your-own-ignore-file-besides-each-repository-s-gitignore-60ecaa",
        &[],
    ),
    setting(
        "core.attributesFile",
        "backend-config-group-files",
        "backend-your-own-attributes-file-besides-each-repository-s-gitattributes-45000c",
        &[],
    ),
    setting(
        "core.hooksPath",
        "backend-config-group-files",
        "backend-the-folder-git-runs-hooks-from-instead-of-git-hooks-66c5eb",
        &[],
    ),
    setting(
        "core.fsmonitor",
        "backend-config-group-performance",
        "backend-whether-a-file-system-monitor-tells-git-what-changed-so-status-ne-976eb8",
        BOOL,
    ),
    setting(
        "core.untrackedCache",
        "backend-config-group-performance",
        "backend-whether-git-remembers-which-folders-have-untracked-files-c599ca",
        BOOL,
    ),
    setting(
        "core.commitGraph",
        "backend-config-group-performance",
        "backend-whether-git-reads-the-commit-graph-file-which-makes-history-faste-7a82f9",
        BOOL,
    ),
    setting(
        "fetch.writeCommitGraph",
        "backend-config-group-performance",
        "backend-whether-every-fetch-updates-the-commit-graph-3c544b",
        BOOL,
    ),
    setting(
        "feature.manyFiles",
        "backend-config-group-performance",
        "backend-turns-on-the-settings-that-help-repositories-with-many-files-70c2b4",
        BOOL,
    ),
    setting(
        "init.defaultBranch",
        "backend-config-group-other",
        "backend-the-name-of-the-first-branch-of-a-new-repository-8c38d9",
        &[],
    ),
    setting(
        "merge.conflictStyle",
        "backend-config-group-other",
        "backend-how-conflicts-are-written-into-files-with-the-common-ancestor-or-f19814",
        &["merge", "diff3", "zdiff3"],
    ),
    setting(
        "rerere.enabled",
        "backend-config-group-other",
        "backend-whether-git-remembers-how-you-resolved-a-conflict-and-resolves-it-e4f0f2",
        BOOL,
    ),
    setting(
        "core.editor",
        "backend-config-group-other",
        "backend-the-editor-git-opens-for-messages-a0e121",
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
            Scope::System => crate::i18n::text("backend-label-d03287ca270c"),
            Scope::Global => crate::i18n::text("backend-label-1037688fd91e"),
            Scope::Local => crate::i18n::text("backend-label-b37f6dba2060"),
            Scope::Worktree => crate::i18n::text("backend-label-e543d993d108"),
            Scope::Command => crate::i18n::text("backend-command-line-c925a1"),
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
    .map_err(|e| {
        crate::i18n::message(
            "backend-could-not-read-the-configuration-e-386381",
            &[("detail", (e).to_string().into())],
        )
    })?;
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
    let option = scope.option().ok_or_else(|| {
        crate::i18n::text("backend-a-command-line-value-can-t-be-written-13a78f").to_owned()
    })?;
    if find(key).is_none() {
        return Err(crate::i18n::message(
            "backend-key-is-not-a-setting-gitcraft-changes-48fcaf",
            &[("key", (key).to_string().into())],
        ));
    }
    if value.contains(['\n', '\r', '\0']) {
        return Err(crate::i18n::text("backend-a-value-can-t-hold-a-line-break-5bde58").to_owned());
    }
    // A file that holds the key twice would refuse a plain set.
    Ok(["config", option, "--replace-all", "--", key, value]
        .map(str::to_owned)
        .to_vec())
}

/// The `git config` arguments that remove every value of `key` from `file` (as git named it),
/// so that what the other files say applies again.
pub fn unset_args(origin: &Origin) -> Result<Vec<String>, String> {
    let file = match (&origin.scope, &origin.file) {
        (Scope::Command, _) | (_, None) => {
            return Err(crate::i18n::text(
                "backend-a-value-given-on-the-command-line-can-t-be-removed-here-38ac0b",
            )
            .to_owned());
        }
        (_, Some(file)) => file,
    };
    Ok(["config", "--file", file, "--unset-all", "--", &origin.key]
        .map(|s| s.to_owned())
        .to_vec())
}

/// Whether `extensions.worktreeConfig` is on, so `--worktree` writes a file of the worktree's
/// own (without it, git writes the repository's file, or refuses with several worktrees).
pub fn worktree_config(repo: &Path) -> bool {
    run(
        Some(repo),
        &["config", "--bool", "--get", "extensions.worktreeConfig"],
    )
    .is_ok_and(|v| v.trim() == "true")
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
mod tests;
