//! Hooks: the folder git runs them from (`core.hooksPath` or `.git/hooks`), which hooks are in
//! it and what runs each, what keeps one from running (not executable, a CRLF `#!` line, an
//! interpreter that isn't installed, a name git doesn't know), and the hook managers that set
//! them up (husky, lefthook, pre-commit). The folder is read in one go, here or on the
//! repository's machine.

use std::path::{Path, PathBuf};

use base64::Engine as _;

use crate::git::run;

/// The hooks git runs, and what runs each.
pub const KNOWN: &[(&str, &str)] = &[
    ("applypatch-msg", "git am"),
    ("pre-applypatch", "git am"),
    ("post-applypatch", "git am"),
    ("pre-commit", "git commit"),
    ("pre-merge-commit", "git merge, pull"),
    (
        "prepare-commit-msg",
        "git commit, merge, cherry-pick, revert, rebase",
    ),
    ("commit-msg", "git commit, merge"),
    ("post-commit", "git commit, cherry-pick, revert, rebase"),
    ("pre-rebase", "git rebase"),
    ("post-checkout", "git checkout, switch, clone, worktree add"),
    ("post-merge", "git merge, pull"),
    ("pre-push", "git push"),
    ("post-rewrite", "git commit --amend, rebase"),
    (
        "reference-transaction",
        "backend-every-change-of-a-ref-a1092b",
    ),
    ("pre-auto-gc", "git gc --auto"),
    (
        "post-index-change",
        "backend-every-write-of-the-index-51ed6f",
    ),
    ("sendemail-validate", "git send-email"),
    ("fsmonitor-watchman", "git status (core.fsmonitor)"),
    (
        "push-to-checkout",
        "backend-a-push-into-this-repository-576e18",
    ),
    (
        "pre-receive",
        "backend-a-push-into-this-repository-server-side-c63be7",
    ),
    (
        "update",
        "backend-a-push-into-this-repository-server-side-c63be7",
    ),
    (
        "proc-receive",
        "backend-a-push-into-this-repository-server-side-c63be7",
    ),
    (
        "post-receive",
        "backend-a-push-into-this-repository-server-side-c63be7",
    ),
    (
        "post-update",
        "backend-a-push-into-this-repository-server-side-c63be7",
    ),
    ("p4-changelist", "git p4 submit"),
    ("p4-prepare-changelist", "git p4 submit"),
    ("p4-post-changelist", "git p4 submit"),
    ("p4-pre-submit", "git p4 submit"),
];

/// What runs the hook `name`, `None` for a name git doesn't run.
pub fn runs_on(name: &str) -> Option<&'static str> {
    KNOWN
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, on)| crate::i18n::text(on))
}

/// Something that keeps a hook from running as meant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Problem {
    /// Git skips it (with a hint) unless it is executable.
    NotExecutable,
    /// Its `#!` line ends in CR: the system looks for `sh\r`, which doesn't exist.
    CrlfShebang,
    /// It has CRLF line endings below its first line: shells read `\r` as part of commands.
    CrlfLines,
    /// No `#!` line: the system can't tell what runs it.
    NoShebang,
    /// The interpreter its `#!` line names isn't there.
    MissingInterpreter(String),
    /// Git runs no hook of that name (a typo, a backup such as `pre-commit.old`).
    UnknownName,
    /// A folder with a hook's name: git tries to run it, and the operation fails.
    IsFolder,
}

impl Problem {
    pub fn describe(&self) -> String {
        match self {
            Problem::NotExecutable => crate::i18n::text(
                "backend-not-executable-git-skips-it-prints-a-hint-and-goes-ahead-without-66d22e",
            )
            .to_owned(),
            Problem::IsFolder => crate::i18n::text(
                "backend-a-folder-with-a-hook-s-name-git-tries-to-run-it-and-the-operation-11b107",
            )
            .to_owned(),
            Problem::CrlfShebang => crate::i18n::text(
                "backend-its-line-ends-in-crlf-the-system-looks-for-an-interpreter-whose-n-74193c",
            )
            .to_owned(),
            Problem::CrlfLines => crate::i18n::text(
                "backend-crlf-line-endings-near-the-top-the-shell-reads-the-carriage-retur-dff8d9",
            )
            .to_owned(),
            Problem::NoShebang => crate::i18n::text(
                "backend-no-line-git-for-windows-can-t-tell-what-runs-it-start-it-with-bin-779e3f",
            )
            .to_owned(),
            Problem::MissingInterpreter(i) => crate::i18n::message(
                "backend-its-interpreter-i-isn-t-installed-where-git-runs-4d4fc2",
                &[("index", (i).to_string().into())],
            ),
            Problem::UnknownName => crate::i18n::text(
                "backend-git-runs-no-hook-of-this-name-it-never-runs-a-typo-or-a-copy-kept-3a6983",
            )
            .to_owned(),
        }
    }
}

/// A file in the hooks folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hook {
    pub name: String,
    /// Where it is when not in the hooks folder itself (`.husky/pre-commit`, which husky's
    /// stub of that name runs).
    pub file: Option<String>,
    /// What runs it, `None` for a name git doesn't know.
    pub runs_on: Option<&'static str>,
    /// The interpreter its `#!` line names (`sh`, `node`, `python3`…).
    pub interpreter: Option<String>,
    pub problems: Vec<Problem>,
}

/// A tool that manages the hooks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Manager {
    Husky,
    Lefthook,
    PreCommit,
}

impl Manager {
    pub fn label(self) -> &'static str {
        match self {
            Manager::Husky => "husky",
            Manager::Lefthook => "lefthook",
            Manager::PreCommit => "pre-commit",
        }
    }

    /// What to change rather than the hooks it writes.
    pub fn advice(self) -> &'static str {
        match self {
            Manager::Husky => crate::i18n::text(
                "backend-the-hooks-in-husky-are-husky-s-stubs-rewritten-on-install-edit-th-583641",
            ),
            Manager::Lefthook => crate::i18n::text(
                "backend-edit-lefthook-yml-lefthook-install-writes-the-hooks-again-9f7ab5",
            ),
            Manager::PreCommit => crate::i18n::text(
                "backend-edit-pre-commit-config-yaml-its-hooks-read-it-each-time-they-run-e85dff",
            ),
        }
    }
}

/// The repository's hooks.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Hooks {
    /// The folder git runs them from, as git names it.
    pub folder: String,
    /// `core.hooksPath` when it is set (else the folder is `.git/hooks`).
    pub hooks_path: Option<String>,
    /// The folder doesn't exist (or isn't a folder).
    pub missing: bool,
    /// `core.hooksPath` is set to nothing: git runs no hook at all.
    pub disabled: bool,
    /// The files git runs as hooks (a hook's name), and the scripts husky's stubs run.
    pub active: Vec<Hook>,
    /// Other files of the folder (`pre-commit.old`, typos): never run.
    pub others: Vec<Hook>,
    /// How many `.sample` files there are (never run).
    pub samples: usize,
    pub managers: Vec<Manager>,
}

/// What is read of each file of the folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scanned {
    pub name: String,
    /// A folder (only listed for a hook's name).
    pub folder: bool,
    pub executable: bool,
    /// Its first bytes.
    pub head: Vec<u8>,
}

/// The repository's hooks, with their problems.
pub fn hooks(repo: &Path) -> Result<Hooks, String> {
    let printed = run(Some(repo), &["rev-parse", "--git-path", "hooks"]).map_err(|e| {
        crate::i18n::message(
            "backend-could-not-find-the-hooks-folder-e-acd76c",
            &[("detail", (e).to_string().into())],
        )
    })?;
    let folder = printed.trim_end_matches(['\n', '\r']).to_owned();
    // Unset (`None`) and set to nothing (hooks turned off) are told apart.
    let hooks_path = run(Some(repo), &["config", "--get", "core.hooksPath"])
        .ok()
        .map(|v| v.trim_end_matches(['\n', '\r']).to_owned());
    if hooks_path.as_deref() == Some("") {
        return Ok(Hooks {
            folder,
            hooks_path,
            disabled: true,
            ..Hooks::default()
        });
    }
    let location = crate::remote::resolve(repo, &folder);
    let scanned = scan(&location)?;
    let missing = scanned.is_none();
    let scanned = scanned.unwrap_or_default();
    let managers = managers(repo, &folder, hooks_path.as_deref(), &scanned);
    // husky's stubs (`.husky/_/pre-commit`) run `.husky/pre-commit`: those are checked too.
    let husky_scripts = if folder.trim_end_matches('/').ends_with(".husky/_") {
        let parent =
            crate::remote::resolve(repo, folder.trim_end_matches('/').trim_end_matches("/_"));
        scan(&parent)?.unwrap_or_default()
    } else {
        Vec::new()
    };
    let interpreters: Vec<String> = scanned
        .iter()
        .chain(&husky_scripts)
        .filter_map(|s| interpreter(&s.head))
        .collect();
    let found = installed(&location, &interpreters);
    let samples = scanned
        .iter()
        .filter(|s| s.name.ends_with(".sample"))
        .count();
    let exec_counts = executable_bit_counts(repo);
    let (active, others) = scanned
        .iter()
        .filter(|s| !s.name.ends_with(".sample") && !helper_file(&s.name))
        .map(|s| check(s, &found, exec_counts))
        .partition(|h| h.runs_on.is_some());
    let mut active: Vec<Hook> = active;
    let husky_dir = folder
        .trim_end_matches('/')
        .trim_end_matches("/_")
        .to_owned();
    for script in husky_scripts
        .iter()
        .filter(|s| runs_on(&s.name).is_some() && !s.folder)
    {
        // husky runs them with `sh -e`: neither the bit nor a #! line matters.
        let mut hook = check(script, &found, false);
        hook.problems.retain(|p| !matches!(p, Problem::NoShebang));
        hook.file = Some(format!("{husky_dir}/{}", script.name));
        active.push(hook);
    }
    Ok(Hooks {
        folder,
        hooks_path,
        missing,
        disabled: false,
        active,
        others,
        samples,
        managers,
    })
}

#[cfg(test)]
mod tests;

mod repair;
#[allow(unused_imports)] // Compatibility re-export.
pub use repair::crlf_to_lf;
#[allow(unused_imports)] // Compatibility re-export.
pub use repair::detect_managers;
pub use repair::make_executable;
use repair::managers;
pub use repair::to_lf;

mod scan;
pub use scan::check;
use scan::executable_bit_counts;
use scan::helper_file;
use scan::installed;
pub use scan::interpreter;
#[allow(unused_imports)] // Compatibility re-export.
pub use scan::parse_scan;
use scan::scan;
