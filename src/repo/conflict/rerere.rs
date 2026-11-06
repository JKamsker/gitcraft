//! Remembered conflict resolutions (`git rerere`): the repository's switch, and a conflicted
//! file that rerere already resolved, opened in the conflict editor with what it reused.

use std::collections::BTreeSet;
use std::path::Path;

use super::{Choice, ConflictEditor, Reused, parse, render, split_resolutions};
use crate::git::{self, InProgress};
use crate::repo::diff::literal_path;
use crate::repo::{RepoTab, ops};

/// What the tab knows of rerere in its repository.
#[derive(Default)]
pub struct Rerere {
    /// Whether rerere is on here (`None` until read).
    pub enabled: Option<bool>,
    /// The files the last operation said it resolved from a recorded resolution.
    pub reused: BTreeSet<String>,
}

/// What the views ask about remembered resolutions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RerereAct {
    /// Turn rerere on or off for this repository.
    SetEnabled(bool),
    /// Forget the resolution reused for a file and bring its conflict back.
    Forget(String),
}

/// Whether rerere is on in the repository at `repo`: `rerere.enabled` when it is set, else
/// whether an `rr-cache` folder exists, as git decides.
pub fn enabled(repo: &Path) -> Result<bool, String> {
    let set = git::run_ok_codes(
        Some(repo),
        &["config", "--type=bool", "--get", "rerere.enabled"],
        &[0, 1],
    )?;
    match set.trim() {
        "true" => return Ok(true),
        "false" => return Ok(false),
        _ => {}
    }
    let cache = git::run(Some(repo), &["rev-parse", "--git-path", "rr-cache"])?;
    let cache = crate::remote::resolve(repo, cache.trim_end_matches(['\n', '\r']));
    Ok(crate::remote::try_is_dir(&cache).unwrap_or(false))
}

/// The git steps that turn rerere on (`true`) or off in the repository. Turned on, a reused
/// resolution is left unstaged (`rerere.autoUpdate` off), whatever the global config says, so
/// that it is checked before it is marked resolved.
pub fn switch_ops(on: bool) -> Vec<Vec<String>> {
    if on {
        ops(&[
            &["config", "--local", "rerere.enabled", "true"],
            &["config", "--local", "rerere.autoUpdate", "false"],
        ])
    } else {
        ops(&[&["config", "--local", "rerere.enabled", "false"]])
    }
}

/// The git steps that forget the resolution reused for `path` (when rerere is `on`) and put the
/// conflict markers back in it.
pub fn forget_ops(path: &str, on: bool) -> Vec<Vec<String>> {
    let path = literal_path(path);
    // Not `--literal-pathspecs` for checkout: it runs the post-checkout hook.
    let redo = ["checkout", "-m", "--", path.as_str()];
    if on {
        ops(&[&["rerere", "forget", "--", path.as_str()], &redo])
    } else {
        ops(&[&redo])
    }
}

/// The files git's output says were resolved from a recorded resolution (`Resolved 'x' using
/// previous resolution.`, or `Staged` when rerere stages them itself).
pub fn reused_in(output: &str) -> BTreeSet<String> {
    output
        .lines()
        .filter_map(|line| {
            let rest = line
                .strip_prefix("Resolved '")
                .or_else(|| line.strip_prefix("Staged '"))?;
            rest.strip_suffix("' using previous resolution.")
                .map(str::to_owned)
        })
        .collect()
}

/// How many files git's output says were resolved from a recorded resolution.
pub fn reused_count(output: &str) -> usize {
    reused_in(output).len()
}

/// The commit merged in by the operation `state` stopped at, as git names it.
fn other_head(state: Option<InProgress>) -> Option<&'static str> {
    match state? {
        InProgress::Merge => Some("MERGE_HEAD"),
        InProgress::CherryPick => Some("CHERRY_PICK_HEAD"),
        InProgress::Revert => Some("REVERT_HEAD"),
        InProgress::Rebase => Some("REBASE_HEAD"),
        InProgress::Am => None,
    }
}

/// The blobs of the three sides of conflicted `path` (base, ours, theirs), from the index.
/// `None` when a side is missing (added on both sides, deleted on one): nothing to rebuild.
fn stages(repo: &Path, path: &str) -> Result<Option<[String; 3]>, String> {
    let out = git::run(
        Some(repo),
        &["--literal-pathspecs", "ls-files", "-u", "-z", "--", path],
    )?;
    let mut sides: [Option<String>; 3] = [None, None, None];
    for entry in out.split('\0') {
        // `<mode> <object> <stage>\t<path>`
        let Some((meta, name)) = entry.split_once('\t') else {
            continue;
        };
        let mut words = meta.split(' ');
        let (Some(_), Some(object), Some(stage)) = (words.next(), words.next(), words.next())
        else {
            continue;
        };
        if name != path {
            continue;
        }
        if let Some(slot) = stage
            .parse::<usize>()
            .ok()
            .and_then(|s| s.checked_sub(1))
            .and_then(|i| sides.get_mut(i))
        {
            *slot = Some(object.to_owned());
        }
    }
    Ok(match sides {
        [Some(base), Some(ours), Some(theirs)] => Some([base, ours, theirs]),
        _ => None,
    })
}

/// Conflicted `path` as git first left it, conflict markers and all (`None`: there is nothing
/// to rebuild, see [`stages`]), rebuilt from the index without touching the file.
pub fn preimage(
    repo: &Path,
    path: &str,
    state: Option<InProgress>,
) -> Result<Option<String>, String> {
    let Some([base, ours, theirs]) = stages(repo, path)? else {
        return Ok(None);
    };
    let other = other_head(state)
        .and_then(|head| {
            git::run(
                Some(repo),
                &["rev-parse", "--verify", "-q", "--short", head],
            )
            .ok()
        })
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "theirs".to_owned());
    // Exits with the number of conflicts; above 127 for an error.
    let ok: Vec<i32> = (0..=127).collect();
    let out = git::run_ok_codes(
        Some(repo),
        &[
            "merge-file",
            "-p",
            "--diff3",
            "--object-id",
            "-L",
            "HEAD",
            "-L",
            "base",
            "-L",
            &other,
            &ours,
            &base,
            &theirs,
        ],
        &ok,
    )
    .map_err(|e| {
        if e.contains("object-id") || e.contains("usage:") {
            "Showing what git rerere reused needs git 2.43 or newer".to_owned()
        } else {
            format!("Could not rebuild the conflicts of {path}: {e}")
        }
    })?;
    Ok(Some(out))
}

impl ConflictEditor {
    /// The editor for conflicted `path`, whose conflict markers are gone (`current`: the file
    /// now) though git still has it unmerged: rerere reused a resolution (`certain` when git
    /// said so), or it was edited elsewhere. Each conflict, rebuilt from `preimage`, shows the
    /// text that now stands in its place.
    pub fn reused(
        path: &str,
        preimage: &str,
        current: String,
        state: Option<InProgress>,
        certain: bool,
    ) -> Option<Self> {
        let (segments, ours_label, theirs_label) = parse(preimage);
        let conflicts = segments
            .iter()
            .filter(|s| matches!(s, super::Segment::Conflict { .. }))
            .count();
        if conflicts == 0 {
            return None;
        }
        let split = split_resolutions(&segments, &current);
        let (choices, recorded) = match &split {
            Some(texts) => (
                vec![Some(Choice::Recorded); conflicts],
                texts.iter().cloned().map(Some).collect(),
            ),
            None => (vec![None; conflicts], vec![None; conflicts]),
        };
        let mut ed = Self {
            path: path.to_owned(),
            segments,
            choices,
            recorded,
            result: current.clone(),
            manual: false,
            ours_label,
            theirs_label,
            rebase: state == Some(InProgress::Rebase),
            solving: false,
            on_disk: 0,
            opened: current,
            generation: 0,
            saving: false,
            reused: Some(if certain {
                Reused::Rerere
            } else {
                Reused::NoMarkers
            }),
        };
        // What can't be told apart conflict by conflict stays as it is, edited by hand.
        let labels = (ed.ours_label.as_str(), ed.theirs_label.as_str());
        ed.manual =
            split.is_none() || render(&ed.segments, &ed.choices, &ed.recorded, labels) != ed.result;
        Some(ed)
    }
}

/// Opens conflicted `path` (in the repository at `repo`) when its markers are gone, with what
/// stands in their place (see [`ConflictEditor::reused`]). Blocking: from the background.
pub fn open_reused(
    repo: &Path,
    path: &str,
    state: Option<InProgress>,
    certain: bool,
) -> Result<Option<ConflictEditor>, String> {
    let file = crate::remote::join(repo, path);
    if super::is_link(&file) {
        return Ok(None);
    }
    let Ok(bytes) = git::worktree_file(&file) else {
        return Ok(None);
    };
    let Ok(current) = String::from_utf8(bytes) else {
        return Ok(None);
    };
    let Some(before) = preimage(repo, path, state)? else {
        return Ok(None);
    };
    Ok(ConflictEditor::reused(
        path, &before, current, state, certain,
    ))
}

impl RepoTab {
    /// Reads whether rerere is on here, in the background.
    pub(in crate::repo) fn read_rerere(&self, ctx: &egui::Context) {
        let repo = self.path.clone();
        self.then(
            ctx,
            move || enabled(&repo),
            |tab, on, _| {
                if let Ok(on) = on {
                    tab.rerere.enabled = Some(on);
                }
            },
        );
    }

    /// Opens conflicted `path` without its markers in the background (see [`open_reused`]),
    /// for the opening numbered `generation`.
    pub(in crate::repo) fn open_reused_later(
        &self,
        generation: u64,
        path: String,
        ctx: &egui::Context,
    ) {
        let (repo, state) = (self.path.clone(), self.work.in_progress);
        let certain = self.rerere.reused.contains(&path);
        self.then(
            ctx,
            {
                let path = path.clone();
                move || open_reused(&repo, &path, state, certain)
            },
            move |tab, found, _| tab.reused_opened(generation, path, found),
        );
    }

    /// Takes the editor for a file without its markers, read for the opening numbered
    /// `generation` (see [`Self::conflict_opened`]); says why it couldn't be read, while that
    /// opening is still the latest.
    pub(in crate::repo) fn reused_opened(
        &mut self,
        generation: u64,
        path: String,
        found: Result<Option<ConflictEditor>, String>,
    ) {
        match found {
            Ok(editor) => self.conflict_opened(generation, path, editor.map(Box::new)),
            Err(e) if generation == self.conflict_generation => self.status = Some((e, true)),
            Err(_) => {}
        }
    }

    /// Notes which files the operation that ended so (`output`) resolved from what was recorded.
    pub(in crate::repo) fn note_reused(&mut self, output: &str) {
        if output.contains("CONFLICT") {
            self.rerere.reused = reused_in(output);
        }
    }

    pub(in crate::repo) fn rerere_act(&mut self, act: RerereAct, ctx: &egui::Context) {
        match act {
            RerereAct::SetEnabled(on) => {
                let repo = self.path.clone();
                self.then(
                    ctx,
                    move || {
                        for step in switch_ops(on) {
                            let args: Vec<&str> = step.iter().map(String::as_str).collect();
                            git::run(Some(&repo), &args)?;
                        }
                        enabled(&repo)
                    },
                    move |tab, now, _| match now {
                        Ok(now) => {
                            tab.rerere.enabled = Some(now);
                            tab.status = Some((
                                if now {
                                    "Conflict resolutions are remembered here from now on"
                                } else {
                                    "Conflict resolutions are no longer remembered here"
                                }
                                .to_owned(),
                                false,
                            ));
                        }
                        Err(e) => {
                            tab.status = Some((
                                format!(
                                    "Could not turn {} remembered resolutions: {e}",
                                    if on { "on" } else { "off" }
                                ),
                                true,
                            ));
                        }
                    },
                );
            }
            RerereAct::Forget(path) => {
                let on = self.rerere.enabled == Some(true);
                self.rerere.reused.remove(&path);
                self.run_ops(
                    &format!("Bring back the conflicts of {path}"),
                    forget_ops(&path, on),
                    crate::repo::Refresh::Status,
                    crate::repo::OpKind::plain(),
                    ctx,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests;
