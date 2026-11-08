//! Custom commands: command lines the user (Settings → Custom commands) or a repository
//! (`.gitcraft/commands.json`, once trusted) adds to a commit's, a branch's or a file's menu, or
//! to the palette. Each runs on the repository's machine, through git's own shell (see
//! [`crate::git::run_shell`]), with what it is about (`$SHA`, `$BRANCH`…) in shell variables:
//! never pasted into its text.

mod program;
mod repo_file;
#[cfg(test)]
mod tests;

use serde::{Deserialize, Serialize};

pub use program::{
    CapturedToast, Scope, captured_program, display_toast_text, terminal_program, toast_text,
    variables,
};
pub use repo_file::{
    FILE as REPO_FILE, MAX_BYTES as REPO_FILE_MAX, RepoFile, TrustedFile, is_trusted, trust,
};
#[cfg(test)]
use repo_file::{digest, parse_repo_file};

/// Where a command is offered.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Placement {
    /// A commit's menu in the graph.
    #[default]
    Commit,
    /// A branch's menu (local or remote).
    Branch,
    /// A file's menu (changes, a commit's files, a diff).
    File,
    /// The command palette.
    Palette,
}

impl Placement {
    pub const ALL: [Placement; 4] = [
        Placement::Commit,
        Placement::Branch,
        Placement::File,
        Placement::Palette,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Placement::Commit => crate::i18n::text("interaction-preferences-commit-menu"),
            Placement::Branch => crate::i18n::text("interaction-preferences-branch-menu"),
            Placement::File => crate::i18n::text("interaction-preferences-file-menu"),
            Placement::Palette => crate::i18n::text("interaction-preferences-palette"),
        }
    }
}

/// Where what a command prints goes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputTo {
    /// It runs in the background; its output (the end of it) shows in a toast.
    #[default]
    Toast,
    /// It runs in a terminal tab of its own, where it can be watched and answered.
    Terminal,
}

impl OutputTo {
    pub const ALL: [OutputTo; 2] = [OutputTo::Toast, OutputTo::Terminal];

    pub fn label(self) -> &'static str {
        match self {
            OutputTo::Toast => crate::i18n::text("interaction-preferences-toast"),
            OutputTo::Terminal => crate::i18n::text("interaction-preferences-terminal"),
        }
    }
}

/// What a prompt asks for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PromptKind {
    /// A line of text.
    #[default]
    Text,
    /// One of the repository's branches (local or remote).
    Branch,
}

impl PromptKind {
    pub const ALL: [PromptKind; 2] = [PromptKind::Text, PromptKind::Branch];

    pub fn label(self) -> &'static str {
        match self {
            PromptKind::Text => crate::i18n::text("interaction-preferences-text"),
            PromptKind::Branch => crate::i18n::text("interaction-preferences-branch"),
        }
    }
}

/// Something asked before the command runs, given to it as `$<variable>`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Prompt {
    /// The shell variable the answer is in (`MESSAGE` → `$MESSAGE`).
    pub variable: String,
    /// What the dialog asks ("Tag name").
    pub label: String,
    pub kind: PromptKind,
    /// What the field starts with.
    pub default: String,
}

/// A command added to a menu or the palette.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CustomCommand {
    pub name: String,
    #[serde(rename = "where")]
    pub placement: Placement,
    /// What `sh` runs, with `$SHA`, `$SHORT_SHA`, `$BRANCH`, `$REMOTE`, `$FILE` and `$REPO` (and
    /// the prompts' variables) in its environment.
    pub command: String,
    pub prompts: Vec<Prompt>,
    /// Ask "Run …?" first.
    pub confirm: bool,
    pub output: OutputTo,
}

impl Default for CustomCommand {
    fn default() -> Self {
        Self {
            name: String::new(),
            placement: Placement::Commit,
            command: String::new(),
            prompts: Vec::new(),
            confirm: true,
            output: OutputTo::Toast,
        }
    }
}

/// The variables gitcraft sets itself, in the order the settings list them.
pub const BUILT_IN: [(&str, &str); 6] = [
    (
        "SHA",
        "the commit's full hash (HEAD outside a commit's menu)",
    ),
    ("SHORT_SHA", "its first 8 characters"),
    (
        "BRANCH",
        "the branch: the menu's (origin/x for a remote one), the commit's pill, else the checked-out one",
    ),
    (
        "REMOTE",
        "that branch's remote: a remote branch's own, a local one's upstream's, else origin",
    ),
    (
        "FILE",
        "the file, relative to the repository (only in a file's menu)",
    ),
    ("REPO", "the repository's folder, on its machine"),
];

/// The built-in variable's prose, resolved at paint time. The variable itself stays literal.
pub fn variable_description<'a>(name: &str, original: &'a str) -> &'a str {
    match name {
        "SHA" => crate::i18n::text("interaction-preferences-variable-sha"),
        "SHORT_SHA" => crate::i18n::text("interaction-preferences-variable-short_sha"),
        "BRANCH" => crate::i18n::text("interaction-preferences-variable-branch"),
        "REMOTE" => crate::i18n::text("interaction-preferences-variable-remote"),
        "FILE" => crate::i18n::text("interaction-preferences-variable-file"),
        "REPO" => crate::i18n::text("interaction-preferences-variable-repo"),
        _ => original,
    }
}

/// Localize only our finite validation messages. Raw validation/error identities stay unchanged.
pub fn display_problem(raw: &str) -> String {
    if raw == "It has no name" {
        return crate::i18n::text("interaction-preferences-missing-name").to_owned();
    }
    if raw == "Its command line is empty" {
        return crate::i18n::text("interaction-preferences-empty-line").to_owned();
    }
    if raw == "It contains a NUL character" {
        return crate::i18n::text("interaction-preferences-nul").to_owned();
    }
    if let Some(variable) = raw.strip_prefix('“').and_then(|s| {
        s.strip_suffix(
            "” isn't a variable name: capital letters, digits and _, starting with a letter",
        )
    }) {
        return crate::i18n::message(
            "interaction-preferences-bad-variable",
            &[("variable", variable.into())],
        );
    }
    if let Some(variable) = raw
        .strip_prefix('$')
        .and_then(|s| s.strip_suffix(" is already set by gitcraft"))
    {
        return crate::i18n::message(
            "interaction-preferences-built-in",
            &[("variable", variable.into())],
        );
    }
    if let Some(variable) = raw
        .strip_prefix('$')
        .and_then(|s| s.strip_suffix(" is used by the shell or git; pick another name"))
    {
        return crate::i18n::message(
            "interaction-preferences-reserved",
            &[("variable", variable.into())],
        );
    }
    if let Some(variable) = raw
        .strip_prefix('$')
        .and_then(|s| s.strip_suffix(" is asked for twice"))
    {
        return crate::i18n::message(
            "interaction-preferences-twice",
            &[("variable", variable.into())],
        );
    }
    raw.to_owned()
}

/// Variables a prompt can't be given: those the shell, git or gitcraft rely on (`PATH`,
/// `IFS`, `HOME`…), which an answer would change for the whole command.
const RESERVED: [&str; 20] = [
    "PATH",
    "HOME",
    "IFS",
    "SHELL",
    "ENV",
    "PWD",
    "OLDPWD",
    "USER",
    "LANG",
    "TERM",
    "CDPATH",
    "PS4",
    // Read-only in bash (`sh` on Git for Windows and macOS), or what tools rely on.
    "UID",
    "EUID",
    "PPID",
    "SHELLOPTS",
    "POSIXLY_CORRECT",
    "TMPDIR",
    "EDITOR",
    "PAGER",
];

/// Prefixes of the variables a prompt can't be given: git's, the loader's, ssh's, gitcraft's.
const RESERVED_PREFIXES: [&str; 7] = ["GIT_", "LD_", "DYLD_", "SSH_", "GITCRAFT_", "LC_", "BASH"];

/// Why `name` can't be a prompt's variable, if it can't: `A`–`Z`, digits and `_`, starting
/// with a letter, at most 32 characters, and none of gitcraft's own nor a reserved one.
pub fn variable_error(name: &str) -> Option<String> {
    let valid = name.len() <= 32
        && name.chars().next().is_some_and(|c| c.is_ascii_uppercase())
        && name
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_');
    if !valid {
        return Some(format!(
            "“{name}” isn't a variable name: capital letters, digits and _, starting with a letter"
        ));
    }
    if BUILT_IN.iter().any(|(n, _)| *n == name) {
        return Some(format!("${name} is already set by gitcraft"));
    }
    if RESERVED.contains(&name) || RESERVED_PREFIXES.iter().any(|p| name.starts_with(p)) {
        return Some(format!(
            "${name} is used by the shell or git; pick another name"
        ));
    }
    None
}

impl CustomCommand {
    /// What is wrong with it, if anything: a command with a problem is listed but not run.
    pub fn problem(&self) -> Option<String> {
        if self.name.trim().is_empty() {
            return Some("It has no name".to_owned());
        }
        if self.command.trim().is_empty() {
            return Some("Its command line is empty".to_owned());
        }
        if self.command.contains('\0') || self.name.contains('\0') {
            return Some("It contains a NUL character".to_owned());
        }
        let mut seen: Vec<&str> = Vec::new();
        for prompt in &self.prompts {
            if let Some(e) = variable_error(&prompt.variable) {
                return Some(e);
            }
            if seen.contains(&prompt.variable.as_str()) {
                return Some(format!("${} is asked for twice", prompt.variable));
            }
            seen.push(&prompt.variable);
        }
        None
    }

    /// The menu or palette entry: its name, with `…` when it asks something first.
    pub fn entry_label(&self) -> String {
        let name = self.name.trim();
        if self.confirm || !self.prompts.is_empty() {
            format!("{name}…")
        } else {
            name.to_owned()
        }
    }
}

/// Where a command offered comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// Settings → Custom commands.
    Global,
    /// The repository's [`REPO_FILE`], trusted.
    Repository,
}

/// A command offered in a menu or the palette, and where it comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Offered {
    pub command: CustomCommand,
    pub source: Source,
}

/// The commands offered at `placement`, from `global` then the repository's (`repo`): only
/// those that can run.
pub fn offered_at<'a>(
    placement: Placement,
    global: &'a [CustomCommand],
    repo: &'a [CustomCommand],
) -> impl Iterator<Item = Offered> + 'a {
    let tagged = global
        .iter()
        .map(|c| (c, Source::Global))
        .chain(repo.iter().map(|c| (c, Source::Repository)));
    tagged
        .filter(move |(c, _)| c.placement == placement && c.problem().is_none())
        .map(|(c, source)| Offered {
            command: c.clone(),
            source,
        })
}
