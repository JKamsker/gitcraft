//! Three-pane conflict editor: each conflict's "ours" and "theirs" side by side with buttons to
//! pick a side (or both), and the resulting file below, editable by hand.

mod save;
use save::is_link;
#[cfg(test)]
use save::save_script;
pub use save::{changed_on_disk, save_solved};
mod syntax;
pub(in crate::repo) use syntax::parse;
use syntax::*;
mod align;
pub mod archaeology;
pub mod decisions;
mod keep_both;
mod keep_both_view;
mod original;
mod original_view;
#[cfg(test)]
mod repo_tests;
pub mod rerere;
mod reused;
mod view;

use std::path::Path;

use crate::git::InProgress;

pub use keep_both_view::KeepBoth;

use self::align::split_resolutions;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::repo) enum Segment {
    Common(String),
    Conflict {
        ours: String,
        base: Option<String>,
        theirs: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    Ours,
    Theirs,
    OursThenTheirs,
    TheirsThenOurs,
    /// The text recorded for the conflict (see [`ConflictEditor::recorded`]).
    Recorded,
}

/// Why a conflicted file opened without conflict markers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reused {
    /// git said it reused a resolution recorded by rerere.
    Rerere,
    /// Its markers are gone though git still has it unmerged (rerere earlier, an editor…).
    NoMarkers,
}

pub struct ConflictEditor {
    pub path: String,
    segments: Vec<Segment>,
    choices: Vec<Option<Choice>>,
    /// A text recorded for each conflict ([`Choice::Recorded`]): what rerere reused.
    recorded: Vec<Option<String>>,
    pub result: String,
    /// The result was edited by hand; choices no longer rewrite it.
    manual: bool,
    ours_label: String,
    theirs_label: String,
    /// During a rebase "ours" is the branch being rebased onto, "theirs" your own commit.
    rebase: bool,
    /// Mergiraf is working on the file.
    pub solving: bool,
    /// How many conflicts the file has on disk, which Mergiraf starts from.
    pub on_disk: usize,
    /// The file as it was opened: while it stays so, a refresh keeps the choices made.
    opened: String,
    /// Which opening of a conflict editor in the tab this is: what was read or checked in the
    /// background for an earlier one (closed since) is not taken for this one.
    pub generation: u64,
    /// Being written on another machine, in the background: neither edited nor saved again
    /// until that is done, so an older save can't land after a newer one.
    pub saving: bool,
    /// Each conflict's original (its base) and the cards that show it.
    original: original::State,
    /// Each side's commits that touched the file, and where the conflict started.
    pub history: archaeology::History,
    /// Opened from a file whose markers are gone: what stands in their place is shown.
    pub reused: Option<Reused>,
    /// The decisions made before for its conflicts, and the notes written for them.
    pub memo: decisions::Memo,
}

impl ConflictEditor {
    /// The file's conflicts as a message for a coding agent: each one's three sides.
    pub(crate) fn for_agent(&self) -> String {
        let conflicts: Vec<crate::agent::handoff::Sides> = self
            .segments
            .iter()
            .filter_map(|s| match s {
                Segment::Conflict { ours, base, theirs } => {
                    Some((ours.as_str(), base.as_deref(), theirs.as_str()))
                }
                Segment::Common(_) => None,
            })
            .collect();
        // The sides as the editor names them (during a rebase, "ours" is upstream's).
        let (ours, theirs) = self.side_names();
        crate::agent::handoff::conflict_message(&self.path, &ours, &theirs, &conflicts)
    }
}

/// Whether `line` (without its line ending) is git's `kind` conflict marker: exactly seven
/// `kind` characters, then nothing or a space and a label. Longer runs (`========` under a
/// Markdown heading, the inner markers of a nested conflict) are content.
impl ConflictEditor {
    /// Opens `path` if it still has conflict markers.
    pub fn open(repo: &Path, path: &str, state: Option<InProgress>) -> Option<Self> {
        // Never a link: a merge can leave one from the other side here, and saving would write
        // through it to wherever it points.
        if is_link(&crate::remote::join(repo, path)) {
            return None;
        }
        let text =
            String::from_utf8(crate::git::worktree_file(&crate::remote::join(repo, path)).ok()?)
                .ok()?;
        let editor = Self::from_text(path, text, state);
        (editor.on_disk > 0).then_some(editor)
    }
    fn from_text(path: &str, text: String, state: Option<InProgress>) -> Self {
        let (segments, ours_label, theirs_label) = parse(&text);
        let conflicts = segments
            .iter()
            .filter(|s| matches!(s, Segment::Conflict { .. }))
            .count();
        Self {
            path: path.to_owned(),
            choices: vec![None; conflicts],
            original: original::State::new(&segments),
            recorded: vec![None; conflicts],
            segments,
            opened: text.clone(),
            result: text,
            manual: false,
            ours_label,
            theirs_label,
            rebase: state == Some(InProgress::Rebase),
            solving: false,
            on_disk: conflicts,
            generation: 0,
            saving: false,
            history: Default::default(),
            reused: None,
            memo: Default::default(),
        }
    }

    pub(crate) fn standalone(text: String) -> Result<Self, String> {
        let editor = Self::from_text("MERGED", text, None);
        if editor.on_disk > 200 {
            return Err(crate::i18n::text("repo-core-3798").into());
        }
        Ok(editor)
    }
    pub(crate) fn resolved(&self) -> bool {
        self.unresolved() == 0
    }

    /// Whether the file in the repository at `repo` is no longer as it was opened (here, one
    /// that can't be read is gone).
    pub fn changed_on_disk(&self, repo: &Path) -> bool {
        changed_on_disk(repo, &self.path, &self.opened) != Some(false)
    }

    /// Whether choices or edits were made here that aren't saved.
    pub fn has_draft(&self) -> bool {
        self.result != self.opened
    }

    /// The file as it was opened (for [`changed_on_disk`] in the background).
    pub fn opened(&self) -> &str {
        &self.opened
    }

    /// Takes `text`, the file with some of its conflicts solved elsewhere (by Mergiraf): the
    /// conflicts left are offered again, the solved ones become text. Returns how many are left.
    pub fn take_solved(&mut self, text: String) -> usize {
        let (segments, ours_label, theirs_label) = parse(&text);
        let left = segments
            .iter()
            .filter(|s| matches!(s, Segment::Conflict { .. }))
            .count();
        if left > 0 {
            self.ours_label = ours_label;
            self.theirs_label = theirs_label;
        }
        self.choices = vec![None; left];
        self.recorded = vec![None; left];
        self.reused = None;
        // Other conflicts: the decisions are read again for them; the notes written for those
        // still there are kept.
        let before = self.decision_spots();
        let memo = std::mem::take(&mut self.memo);
        self.segments = segments;
        self.original.reset(&self.segments);
        self.result = text;
        self.manual = false;
        self.carry_drafts(&before, memo);
        left
    }

    /// Whether the index is still to be asked for the bases of the conflicts.
    pub fn wants_bases(&self) -> bool {
        self.original.index == original::IndexBases::Loading
    }

    /// Asks the index of `repo` for the bases of the conflicts, here and now (as the
    /// background job does).
    #[cfg(test)]
    pub fn read_bases(&mut self, repo: &Path) {
        if self.wants_bases() {
            let bases = original::load(repo, &self.path);
            self.original.set_index(bases, &self.segments);
        }
    }

    fn choose(&mut self, k: usize, choice: Choice) {
        if let Some(c) = self.choices.get_mut(k) {
            *c = if *c == Some(choice) {
                None
            } else {
                Some(choice)
            };
        }
        // Edited by hand: the choice waits for "Back to choices" rather than undo the edits.
        if !self.manual {
            self.result = self.rendered();
        }
    }

    /// The file for the choices made.
    fn rendered(&self) -> String {
        render(
            &self.segments,
            &self.choices,
            &self.recorded,
            (&self.ours_label, &self.theirs_label),
        )
    }

    /// The text recorded for conflict `k`, if any.
    #[cfg(test)]
    fn recorded(&self, k: usize) -> Option<&str> {
        self.recorded.get(k).and_then(Option::as_deref)
    }

    /// The conflicts left in the result, by its markers (also after editing it by hand).
    fn unresolved(&self) -> usize {
        unresolved_in(&self.result, &self.segments)
    }

    pub fn save(&self, repo: &Path) -> Result<(), String> {
        let text = self.solved()?;
        save_solved(repo, &self.path, &text)
    }

    /// The file with every conflict solved, as it is to be saved; or how many are left.
    pub fn solved(&self) -> Result<String, String> {
        let left = self.unresolved();
        if left > 0 {
            return Err(crate::i18n::message(
                "repo-core-3799",
                &[
                    ("left", (left).into()),
                    ("path", self.path.to_string().into()),
                ],
            ));
        }
        Ok(self.result.clone())
    }

    /// Names for the two sides as the user thinks of them.
    fn side_names(&self) -> (String, String) {
        if self.rebase {
            (
                crate::i18n::message(
                    "repo-core-3800",
                    &[("ours-label", short(&self.ours_label).to_string().into())],
                ),
                crate::i18n::message(
                    "repo-core-3801",
                    &[("theirs-label", short(&self.theirs_label).to_string().into())],
                ),
            )
        } else {
            (
                crate::i18n::message(
                    "repo-core-3802",
                    &[("ours-label", short(&self.ours_label).to_string().into())],
                ),
                crate::i18n::message(
                    "repo-core-3803",
                    &[("theirs-label", short(&self.theirs_label).to_string().into())],
                ),
            )
        }
    }
}

fn short(label: &str) -> String {
    let s = label.split_whitespace().next().unwrap_or(label);
    if s.chars().count() > 28 {
        format!("{}…", s.chars().take(27).collect::<String>())
    } else {
        s.to_owned()
    }
}

#[cfg(test)]
mod tests;
