//! Starting a diff or merge tool on a repository's file and telling how it ended. Everything
//! goes through git (`git difftool`, `git mergetool`) on the repository's machine, and blocks
//! until the tool closes: call it off the window's thread.

use std::path::Path;

use super::plan::{self, Place, Plan};
use super::preset::{Kind, Preset, ToolChoice};
use super::target::DiffTarget;
use crate::git::{ToolRun, parse_name_status, run, run_tool};
use crate::remote::{self, Host};

/// How a merge tool left the file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MergeOutcome {
    /// The tool said it worked (or saved the file, for one whose exit code isn't trusted): git
    /// staged the file, so it is resolved.
    Resolved,
    /// The tool was closed without a result, or reported a failure: still conflicted. Why.
    Unresolved(String),
}

/// Where git (and so the tool) runs for the repository at `repo`.
pub fn place_of(repo: &Path) -> Place {
    match remote::parse(repo).map(|t| t.host) {
        Some(Host::Wsl(_)) => Place::Wsl,
        Some(Host::Ssh(host)) => Place::Ssh(host),
        None => Place::Here,
    }
}

/// Shows `target` in the diff tool `choice` names, and returns once it is closed. An error says
/// what went wrong (no tool set, the tool not installed, nothing to compare…).
pub fn open_diff(repo: &Path, choice: &ToolChoice, target: &DiffTarget) -> Result<(), String> {
    let rest = target.args()?;
    if !has_differences(repo, &rest)? {
        return Err(format!(
            "{} is the same on both sides: there is nothing to compare",
            target.path()
        ));
    }
    let plan = plan_for(repo, choice, Kind::Diff)?;
    let ran = start(repo, &plan, Kind::Diff, &rest)?;
    if ran.code == Some(0) {
        Ok(())
    } else {
        Err(format!(
            "{} could not show {}: {}",
            plan.label,
            target.path(),
            failure(&ran, Kind::Diff, &plan)
        ))
    }
}

/// Opens the conflicted file `path` in the merge tool `choice` names, and returns once it is
/// closed: whether the file is resolved now. git decides, as it does in a terminal: with
/// `mergetool.<tool>.trustExitCode` from the tool's exit code, otherwise from whether the file
/// was saved; `mergetool.keepBackup` says whether a `.orig` copy is kept.
pub fn resolve_conflict(
    repo: &Path,
    choice: &ToolChoice,
    path: &str,
) -> Result<MergeOutcome, String> {
    mergeable(repo, path)?;
    let plan = plan_for(repo, choice, Kind::Merge)?;
    let rest = ["--".to_owned(), path.to_owned()];
    let ran = start(repo, &plan, Kind::Merge, &rest)?;
    if !unmerged_stages(repo, path)?.is_empty() {
        let what = failure(&ran, Kind::Merge, &plan);
        if not_available(&ran) {
            return Err(format!("{} could not open {path}: {what}", plan.label));
        }
        let why = if said(&ran, "seems unchanged") {
            format!("{} was closed without saving it", plan.label)
        } else {
            format!("{} didn't report success ({what})", plan.label)
        };
        return Ok(MergeOutcome::Unresolved(format!(
            "{path} is still conflicted: {why}"
        )));
    }
    Ok(MergeOutcome::Resolved)
}

/// Whether the comparison `rest` (see [`DiffTarget::args`]) finds any difference.
fn has_differences(repo: &Path, rest: &[String]) -> Result<bool, String> {
    let mut args = vec!["--literal-pathspecs", "diff", "--name-only"];
    args.extend(rest.iter().map(String::as_str));
    Ok(!run(Some(repo), &args)?.trim().is_empty())
}

/// Runs the tool `plan` starts, `rest` after git's command.
fn start(repo: &Path, plan: &Plan, kind: Kind, rest: &[String]) -> Result<ToolRun, String> {
    let args = plan::tool_args(plan, kind, rest);
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    run_tool(repo, &args)
}

/// How to start the tool `choice` names for the repository at `repo`.
fn plan_for(repo: &Path, choice: &ToolChoice, kind: Kind) -> Result<Plan, String> {
    let Some(preset) = choice.preset else {
        return plan::configured(&configured_name(repo, kind), kind);
    };
    let place = place_of(repo);
    let program = program_for(preset, &choice.program, &place)?;
    let trust = match kind {
        Kind::Merge => trust_setting(repo, preset),
        Kind::Diff => None,
    };
    Ok(plan::preset(
        preset,
        kind,
        &place,
        program.as_deref(),
        trust,
    ))
}

/// The tool git's config names for `kind`: `diff.tool` falls back to `merge.tool`, as in git.
fn configured_name(repo: &Path, kind: Kind) -> String {
    let get = |key: &str| {
        run(Some(repo), &["config", "--get", key])
            .ok()
            .map(|v| v.trim().to_owned())
            .filter(|v| !v.is_empty())
    };
    match kind {
        Kind::Diff => get("diff.tool").or_else(|| get("merge.tool")),
        Kind::Merge => get("merge.tool"),
    }
    .unwrap_or_default()
}

/// What `mergetool.<tool>.trustExitCode` says for the preset, under git's own name for it.
fn trust_setting(repo: &Path, preset: Preset) -> Option<bool> {
    let key = format!(
        "mergetool.{}.trustExitCode",
        preset.git_name().unwrap_or(preset.id())
    );
    match run(Some(repo), &["config", "--get", "--bool", &key])
        .ok()?
        .trim()
    {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

/// The program to start the preset with: the one set in Settings; else, where it has to be a
/// program of this Windows machine (WSL) or git doesn't look in Program Files (Windows), where
/// its installer put it; else the one git finds by itself.
fn program_for(preset: Preset, typed: &str, place: &Place) -> Result<Option<String>, String> {
    let typed = typed.trim();
    if !typed.is_empty() {
        return Ok(Some(typed.to_owned()));
    }
    let installed = || {
        plan::windows_program(
            preset,
            |var| std::env::var(var).ok(),
            |path| Path::new(path).is_file(),
        )
    };
    match place {
        Place::Wsl => installed().map(Some).ok_or_else(|| {
            format!(
                "{} wasn't found in Program Files. Set where it is (its .exe) in Settings → Git \
                 → External tools.",
                preset.label()
            )
        }),
        Place::Here if cfg!(windows) => Ok(installed()),
        _ => Ok(None),
    }
}

/// Checks that `path` is a conflict a merge tool can take: both sides changed its content.
/// A file deleted on one side, a symbolic link or a submodule would have git ask a question
/// on a terminal nobody sees.
fn mergeable(repo: &Path, path: &str) -> Result<(), String> {
    let stages = unmerged_stages(repo, path)?;
    if stages.is_empty() {
        return Err(format!("{path} has no conflict left to resolve"));
    }
    let side = |n: u8| stages.iter().find(|(_, stage)| *stage == n);
    let (Some(ours), Some(theirs)) = (side(2), side(3)) else {
        return Err(format!(
            "{path} was deleted on one side: keep it or delete it with Use mine / Use theirs, \
             or in the terminal, rather than in a merge tool"
        ));
    };
    for (mode, _) in [ours, theirs] {
        let what = match mode.as_str() {
            "120000" => "a symbolic link",
            "160000" => "a submodule",
            _ => continue,
        };
        return Err(format!(
            "{path} is {what} on one side: a merge tool can only merge the contents of files"
        ));
    }
    Ok(())
}

/// The conflict stages git holds for `path`: (mode, stage) of each.
fn unmerged_stages(repo: &Path, path: &str) -> Result<Vec<(String, u8)>, String> {
    let out = run(
        Some(repo),
        &["--literal-pathspecs", "ls-files", "-u", "-z", "--", path],
    )?;
    Ok(parse_unmerged(&out, path))
}

/// `ls-files -u -z`: `mode sha stage\tpath\0` for each stage; only those of `path` itself.
fn parse_unmerged(out: &str, path: &str) -> Vec<(String, u8)> {
    out.split('\0')
        .filter_map(|entry| {
            let (info, name) = entry.split_once('\t')?;
            if name != path {
                return None;
            }
            let mut parts = info.split(' ');
            let mode = parts.next()?.to_owned();
            let stage = parts.nth(1)?.parse().ok()?;
            Some((mode, stage))
        })
        .collect()
}

/// The file a range renamed `path` from, if it did.
pub(super) fn renamed_from(
    repo: &Path,
    from: &str,
    to: Option<&str>,
    path: &str,
) -> Option<String> {
    let mut args = vec!["diff", "-M", "--name-status", "-z", from];
    args.extend(to);
    args.push("--");
    let out = run(Some(repo), &args).ok()?;
    parse_name_status(&out)
        .into_iter()
        .find(|f| f.path == path)
        .and_then(|f| f.old_path)
}

/// The first parent of `hash`, or the empty tree for a commit with none, to compare it with.
pub(super) fn parent_of(repo: &Path, hash: &str) -> Result<String, String> {
    let line = run(
        Some(repo),
        &["rev-list", "--parents", "-n", "1", hash, "--"],
    )?;
    match line.split_whitespace().nth(1) {
        Some(parent) => Ok(parent.to_owned()),
        // `--stdin` with nothing on it: the empty tree, in whatever hash the repository uses.
        None => Ok(run(Some(repo), &["hash-object", "-t", "tree", "--stdin"])?
            .trim()
            .to_owned()),
    }
}

/// Whether git said `words` (in either output).
fn said(ran: &ToolRun, words: &str) -> bool {
    ran.stderr.contains(words) || ran.stdout.contains(words)
}

/// Whether git couldn't start the tool at all.
fn not_available(ran: &ToolRun) -> bool {
    said(ran, "is not available as") || said(ran, "Unknown merge tool")
}

/// What git said about a tool that failed, in a line or two, with what to do about a tool
/// that isn't there.
fn failure(ran: &ToolRun, kind: Kind, plan: &Plan) -> String {
    let lines: Vec<&str> = ran
        .stderr
        .lines()
        .chain(ran.stdout.lines())
        .map(str::trim)
        .filter(|l| {
            !l.is_empty()
                && !l.starts_with("Normal merge conflict")
                && !l.starts_with("{local}")
                && !l.starts_with("{remote}")
                && !l.starts_with("Merging:")
                && !l.starts_with("Was the merge successful")
        })
        .collect();
    let said = match lines.as_slice() {
        [] => match ran.code {
            Some(code) => format!("git {} exited with code {code}", kind.command()),
            None => format!("git {} was stopped", kind.command()),
        },
        lines => lines[..lines.len().min(2)].join(" "),
    };
    if not_available(ran) && plan.name != plan.label {
        format!("{said}. Install it, or set where it is in Settings → Git → External tools.")
    } else {
        said
    }
}
