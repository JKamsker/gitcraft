//! How git is told to start a tool: the `--tool` name and the `-c` settings that go with it.
//! Nothing here runs anything, so every way of starting one can be checked as it is built.

use super::preset::{Kind, Preset, Word};
use crate::remote::sh_quote;

/// Where git runs, and so where the tool it starts runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Place {
    /// This machine.
    Here,
    /// A WSL distribution: git runs there, the tool is a Windows program of this machine, handed
    /// the files as `\\wsl.localhost` paths.
    Wsl,
    /// Over SSH: git and the tool run on that machine (named so).
    Ssh(String),
}

/// What `git difftool` / `git mergetool` is run with to start a tool.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    /// The `--tool` name.
    pub name: String,
    /// Settings for this run only (`-c key=value`), never written to a config file.
    pub config: Vec<(String, String)>,
    /// What the tool is called in messages.
    pub label: String,
}

/// Tools that need a terminal, which a tool started from the window doesn't have: they would
/// wait for keys nobody can type.
fn needs_terminal(name: &str) -> bool {
    ["vimdiff", "nvimdiff"]
        .iter()
        .any(|t| name.strip_prefix(t).is_some_and(|rest| rest.len() <= 1))
}

/// The tool git's config names (`diff.tool`, `merge.tool`), as it is.
pub fn configured(name: &str, kind: Kind) -> Result<Plan, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(no_tool_set(kind));
    }
    if needs_terminal(name) {
        return Err(format!(
            "{name} runs in a terminal, which a tool started from the window doesn't have. Run \
             git {} in the terminal panel, or choose a tool with a window in Settings → Git → \
             External tools.",
            kind.command()
        ));
    }
    Ok(Plan {
        name: name.to_owned(),
        config: Vec::new(),
        label: name.to_owned(),
    })
}

/// What to say when git's config names no tool.
pub fn no_tool_set(kind: Kind) -> String {
    format!(
        "No {noun} tool is set: choose one in Settings → Git → External tools, or set {key} in \
         git's config (git config --global {key} <tool>).",
        noun = kind.noun(),
        key = kind.key(),
    )
}

/// A preset, started in `place`. `program`: the program to run when it isn't the one git
/// finds by itself (for WSL, a Windows path); `trust`: what `mergetool.<tool>.trustExitCode`
/// says, if anything.
pub fn preset(
    preset: Preset,
    kind: Kind,
    place: &Place,
    program: Option<&str>,
    trust: Option<bool>,
) -> Plan {
    let label = preset.label().to_owned();
    match preset.git_name() {
        // git's own definition: it finds the program, knows its options and when to trust it.
        Some(git_name) if *place != Place::Wsl => Plan {
            name: git_name.to_owned(),
            config: program
                .map(|p| (format!("{}.{git_name}.path", kind.command()), p.to_owned()))
                .into_iter()
                .collect(),
            label,
        },
        _ => {
            let name = format!("gitcraft-{}", preset.id());
            let program = program.unwrap_or(preset.default_program());
            let words = match kind {
                Kind::Diff => preset.diff_words(),
                Kind::Merge => preset.merge_words(),
            };
            let script = if *place == Place::Wsl && is_windows_path(program) {
                wsl_script(program, words)
            } else {
                format!(
                    "{AS_FILE}; {} {}",
                    sh_quote(program),
                    shell_words(words, false)
                )
            };
            let mut config = vec![(format!("{}.{name}.cmd", kind.command()), script)];
            if kind == Kind::Merge {
                let trust = trust.unwrap_or(preset.trusts_exit_code());
                config.push((format!("mergetool.{name}.trustExitCode"), trust.to_string()));
            }
            Plan {
                name,
                config,
                label,
            }
        }
    }
}

/// A `sh` function giving a file name git hands over (relative to the repository's top for the
/// working tree's side) as one no program reads as an option: `-x.txt` becomes `./-x.txt`.
pub(super) const AS_FILE: &str =
    "gc_a() { case \"$1\" in -*) printf './%s' \"$1\";; *) printf '%s' \"$1\";; esac; }";

/// `words` as `sh` reads them, each file git hands over in its variable: through `gc_a` (see
/// [`AS_FILE`]) or, in WSL, turned into the `\\wsl.localhost` path a Windows program opens
/// (through `gc_w`, see [`wsl_script`]). A file glued to an option (`--output=`) can't be read
/// as one, and is left as it is but in WSL.
fn shell_words(words: &[Word], wsl: bool) -> String {
    words
        .iter()
        .map(|w| match w {
            Word::Text(text) => sh_quote(text),
            Word::File(prefix, slot) => {
                let var = format!("\"${}\"", slot.var());
                match (wsl, prefix.is_empty()) {
                    (true, _) => format!("{}\"$(gc_w {var})\"", sh_quote_or_empty(prefix)),
                    (false, true) => format!("\"$(gc_a {var})\""),
                    (false, false) => format!("{}{var}", sh_quote(prefix)),
                }
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// `s` quoted for `sh`, or nothing for an empty `s`.
fn sh_quote_or_empty(s: &str) -> String {
    if s.is_empty() {
        String::new()
    } else {
        sh_quote(s)
    }
}

/// Whether `program` is a Windows path (`C:\\…`, `\\\\server\\…`): a program of this machine,
/// not one inside WSL (`/usr/bin/meld`, `code`), which takes the files as they are there.
fn is_windows_path(program: &str) -> bool {
    program.contains('\\') || program.as_bytes().get(1) == Some(&b':')
}

/// The command a tool of this (Windows) machine runs with from git in WSL. `gc_w` gives the
/// Windows path of each file: git's `/dev/null` (a file new or deleted on one side) and a base
/// that isn't there become an empty file, as a Windows program can open neither. The program
/// itself is a Windows path, made a path there (`wslpath -u`).
fn wsl_script(program: &str, words: &[Word]) -> String {
    let program = format!("\"$(wslpath -u {})\"", sh_quote(program));
    let helper = "gc_w() { case \"$1\" in /dev/null) gc_p=\"${TMPDIR:-/tmp}/gitcraft-empty-file\"; \
                  : > \"$gc_p\";; /*) gc_p=\"$1\";; *) gc_p=\"$PWD/$1\";; esac; \
                  [ -e \"$gc_p\" ] || : > \"$gc_p\"; wslpath -w \"$gc_p\"; }";
    format!("{helper}; {program} {}", shell_words(words, true))
}

/// The whole git command: the plan's settings, `git <difftool|mergetool> --tool=<name> -y`, then
/// `rest` (what to compare, or the file to merge). Pathspecs are literal: a file named `*.rs`
/// is only that file.
pub fn tool_args(plan: &Plan, kind: Kind, rest: &[String]) -> Vec<String> {
    let mut args = Vec::new();
    for (key, value) in &plan.config {
        args.push("-c".to_owned());
        args.push(format!("{key}={value}"));
    }
    args.extend([
        "--literal-pathspecs".to_owned(),
        kind.command().to_owned(),
        format!("--tool={}", plan.name),
        "-y".to_owned(),
    ]);
    args.extend(rest.iter().cloned());
    args
}

/// Where Windows installed `preset`, given the environment's folders (`env`) and whether a
/// file is there (`exists`).
pub fn windows_program(
    preset: Preset,
    env: impl Fn(&str) -> Option<String>,
    exists: impl Fn(&str) -> bool,
) -> Option<String> {
    preset.windows_places().iter().find_map(|(var, rest)| {
        let base = env(var).filter(|b| !b.trim().is_empty())?;
        let path = format!("{}\\{rest}", base.trim_end_matches(['\\', '/']));
        exists(&path).then_some(path)
    })
}
