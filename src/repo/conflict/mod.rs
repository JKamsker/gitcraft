//! Three-pane conflict editor: each conflict's "ours" and "theirs" side by side with buttons to
//! pick a side (or both), and the resulting file below, editable by hand.

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
enum Segment {
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
fn is_marker(line: &str, kind: char) -> bool {
    let mut chars = line.chars();
    (&mut chars).take(7).filter(|&c| c == kind).count() == 7
        && matches!(chars.next(), None | Some(' '))
}

/// Splits a file with conflict markers into common text and conflicts (diff3 bases kept).
/// A conflict that never ends (no `>>>>>>>` before the end of the file) stays text.
fn parse(text: &str) -> (Vec<Segment>, String, String) {
    enum State {
        Common,
        Ours,
        Base,
        Theirs,
    }
    let mut segments = Vec::new();
    let (mut common, mut ours, mut base, mut theirs) =
        (String::new(), String::new(), None::<String>, String::new());
    let (mut ours_label, mut theirs_label) = (String::new(), String::new());
    // The lines of the conflict being read, markers included, in case it never ends.
    let mut raw = String::new();
    let mut state = State::Common;
    for line in text.split_inclusive('\n') {
        let bare = line.trim_end_matches(['\r', '\n']);
        if !matches!(state, State::Common) {
            raw.push_str(line);
        }
        match state {
            State::Common if is_marker(bare, '<') => {
                raw.clear();
                raw.push_str(line);
                if !common.is_empty() {
                    segments.push(Segment::Common(std::mem::take(&mut common)));
                }
                if ours_label.is_empty() {
                    ours_label = bare.trim_start_matches('<').trim().to_owned();
                }
                state = State::Ours;
            }
            State::Common => common.push_str(line),
            State::Ours if is_marker(bare, '|') => {
                base = Some(String::new());
                state = State::Base;
            }
            State::Ours | State::Base if is_marker(bare, '=') => state = State::Theirs,
            State::Ours => ours.push_str(line),
            State::Base => {
                if let Some(b) = &mut base {
                    b.push_str(line);
                }
            }
            State::Theirs if is_marker(bare, '>') => {
                if theirs_label.is_empty() {
                    theirs_label = bare.trim_start_matches('>').trim().to_owned();
                }
                raw.clear();
                segments.push(Segment::Conflict {
                    ours: std::mem::take(&mut ours),
                    base: base.take(),
                    theirs: std::mem::take(&mut theirs),
                });
                state = State::Common;
            }
            State::Theirs => theirs.push_str(line),
        }
    }
    if !matches!(state, State::Common) {
        // Not a conflict after all: keep the text as it was rather than lose it.
        common.push_str(&raw);
    }
    if !common.is_empty() {
        segments.push(Segment::Common(common));
    }
    (segments, ours_label, theirs_label)
}

/// The file for the current choices (`recorded`: the text of each [`Choice::Recorded`]);
/// undecided conflicts keep their markers.
fn render(
    segments: &[Segment],
    choices: &[Option<Choice>],
    recorded: &[Option<String>],
    labels: (&str, &str),
) -> String {
    let mut out = String::new();
    let mut k = 0;
    for seg in segments {
        match seg {
            Segment::Common(text) => out.push_str(text),
            Segment::Conflict { ours, base, theirs } => {
                let recorded = recorded.get(k).and_then(Option::as_deref);
                match choices.get(k).copied().flatten() {
                    Some(Choice::Recorded) if recorded.is_some() => {
                        out.push_str(recorded.unwrap_or_default());
                    }
                    Some(Choice::Ours) => out.push_str(ours),
                    Some(Choice::Theirs) => out.push_str(theirs),
                    Some(Choice::OursThenTheirs) => {
                        out.push_str(ours);
                        out.push_str(theirs);
                    }
                    Some(Choice::TheirsThenOurs) => {
                        out.push_str(theirs);
                        out.push_str(ours);
                    }
                    None | Some(Choice::Recorded) => {
                        out.push_str(&format!("<<<<<<< {}\n", labels.0));
                        out.push_str(ours);
                        if let Some(base) = base {
                            out.push_str("||||||| base\n");
                            out.push_str(base);
                        }
                        out.push_str("=======\n");
                        out.push_str(theirs);
                        out.push_str(&format!(">>>>>>> {}\n", labels.1));
                    }
                }
                k += 1;
            }
        }
    }
    out
}

/// The lines of `text` that look like one of git's conflict markers.
fn marker_lines(text: &str) -> impl Iterator<Item = &str> {
    text.lines()
        .map(|l| l.trim_end_matches('\r'))
        .filter(|l| ['<', '|', '=', '>'].iter().any(|&kind| is_marker(l, kind)))
}

/// How many conflicts are left in `result`: each whole one, and what is left of those taken
/// apart by hand (a marker deleted, the others kept). `segments`: the file as it was parsed,
/// whose marker-like lines that are text (a start marker that never closed, a heading's
/// `=======` on one side) may stay.
fn unresolved_in(result: &str, segments: &[Segment]) -> usize {
    use std::collections::HashMap;
    let mut text_markers: HashMap<&str, usize> = HashMap::new();
    for seg in segments {
        let texts = match seg {
            Segment::Common(text) => [Some(text), None, None],
            Segment::Conflict { ours, base, theirs } => [Some(ours), base.as_ref(), Some(theirs)],
        };
        for line in texts.into_iter().flatten().flat_map(|t| marker_lines(t)) {
            *text_markers.entry(line).or_default() += 1;
        }
    }
    let (now, _, _) = parse(result);
    let mut whole = 0;
    // Marker lines outside a whole conflict that the file didn't have as text, by kind.
    let (mut starts, mut middles, mut ends) = (0, 0, 0);
    for seg in &now {
        let text = match seg {
            Segment::Conflict { .. } => {
                whole += 1;
                continue;
            }
            Segment::Common(text) => text,
        };
        for line in marker_lines(text) {
            match text_markers.get_mut(line) {
                Some(n) if *n > 0 => *n -= 1,
                _ if line.starts_with('<') => starts += 1,
                _ if line.starts_with('>') => ends += 1,
                _ => middles += 1,
            }
        }
    }
    // A conflict taken apart keeps its start or its end marker, or at least a middle one.
    whole + starts.max(ends).max(usize::from(middles > 0))
}

/// Whether `file` is a symbolic link (not what it points to).
fn is_link(file: &Path) -> bool {
    crate::remote::is_link(file)
}

/// Writes `text`, the conflicts of `path` (in the repository at `repo`) solved, over it. On
/// another machine, a round trip: in the background.
pub fn save_solved(repo: &Path, path: &str, text: &str) -> Result<(), String> {
    let file = crate::remote::join(repo, path);
    if let Some(target) = crate::remote::parse(&file) {
        return save_there(&target, text).map_err(|e| {
            crate::i18n::message(
                "repo-core-3786",
                &[
                    ("path", path.to_string().into()),
                    ("e", e.to_string().into()),
                ],
            )
        });
    }
    // A link put there since it was opened is not written through.
    if is_link(&file) {
        return Err(crate::i18n::message(
            "repo-core-3787",
            &[("path", path.to_string().into())],
        ));
    }
    std::fs::write(&file, text).map_err(|e| {
        crate::i18n::message(
            "repo-core-3786",
            &[
                ("path", path.to_string().into()),
                ("e", format!("{}", e).into()),
            ],
        )
    })
}

/// What the save script ends with when the file is a link, or not a file, now.
const NOT_A_PLAIN_FILE: i32 = 4;

/// The script that saves a solved file on another machine, from its input: written to a new
/// file beside it (its mode copied), then renamed over it. A rename replaces whatever is at
/// the path, never follows it: a link put there after the check is replaced, not written
/// through.
fn save_script(file: &str) -> String {
    let f = crate::remote::sh_path(file);
    let dir = crate::remote::sh_path(crate::remote::parent_there(file));
    format!(
        concat!(
            "f={f}\n",
            "if [ -L \"$f\" ] || [ ! -f \"$f\" ]; then exit {not_plain}; fi\n",
            "t=$(mktemp {dir}/.gitcraft-save.XXXXXX) || exit 1\n",
            "trap 'rm -f \"$t\"' EXIT\n",
            "cat > \"$t\" || exit 1\n",
            "m=$(stat -c %a -- \"$f\" 2>/dev/null || stat -f %Lp -- \"$f\" 2>/dev/null)\n",
            "chmod \"${{m:-644}}\" \"$t\" || exit 1\n",
            "mv -f -- \"$t\" \"$f\"\n",
        ),
        f = f,
        dir = dir,
        not_plain = NOT_A_PLAIN_FILE,
    )
}

/// [`save_solved`] on another machine (blocking: from a thread of its own).
fn save_there(target: &crate::remote::Target, text: &str) -> Result<(), String> {
    let out = crate::remote::exec(
        &target.host,
        &crate::remote::Request {
            script: save_script(&target.path),
            stdin: Some(text.as_bytes().to_vec()),
            ..crate::remote::Request::default()
        },
    )?;
    match out.code {
        0 => Ok(()),
        NOT_A_PLAIN_FILE => Err(crate::i18n::text("repo-core-3796").to_owned()),
        _ => Err(String::from_utf8_lossy(&out.stderr).trim().to_owned()),
    }
}

/// Whether `path` (in the repository at `repo`) is no longer `opened`, as it was opened:
/// `None` when it couldn't be read (on another machine, a dropped connection says nothing
/// about the file). On another machine, a round trip: in the background.
pub fn changed_on_disk(repo: &Path, path: &str, opened: &str) -> Option<bool> {
    let bytes = crate::git::worktree_file(&crate::remote::join(repo, path)).ok()?;
    Some(String::from_utf8(bytes).ok().as_deref() != Some(opened))
}

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
mod tests {
    use super::*;

    const FILE: &str = "a\n<<<<<<< HEAD\nours 1\n=======\ntheirs 1\n>>>>>>> feature\nb\n<<<<<<< HEAD\nx\n||||||| base\nbase\n=======\ny\n>>>>>>> feature\nc\n";

    #[test]
    fn an_agent_is_told_every_conflict_with_its_three_sides() {
        let msg = editor(FILE).for_agent();
        assert!(msg.contains("has 2 merge conflicts"), "{msg}");
        assert!(
            msg.contains("ours (Ours (HEAD)):\n```\nours 1\n```"),
            "{msg}"
        );
        assert!(
            msg.contains("base (what both started from):\n```\nbase\n```"),
            "{msg}"
        );
        assert!(
            msg.contains("theirs (Theirs (feature)):\n```\ny\n```"),
            "{msg}"
        );
    }

    #[test]
    fn parses_conflicts_and_labels() {
        let (segs, ours, theirs) = parse(FILE);
        assert_eq!((ours.as_str(), theirs.as_str()), ("HEAD", "feature"));
        let conflicts: Vec<_> = segs
            .iter()
            .filter(|s| matches!(s, Segment::Conflict { .. }))
            .collect();
        assert_eq!(conflicts.len(), 2);
        assert_eq!(
            conflicts[1],
            &Segment::Conflict {
                ours: "x\n".into(),
                base: Some("base\n".into()),
                theirs: "y\n".into()
            }
        );
    }

    #[test]
    fn renders_choices_and_keeps_undecided_markers() {
        let (segs, o, t) = parse(FILE);
        let out = render(
            &segs,
            &[Some(Choice::Theirs), Some(Choice::OursThenTheirs)],
            &[],
            (&o, &t),
        );
        assert_eq!(out, "a\ntheirs 1\nb\nx\ny\nc\n");
        let partial = render(&segs, &[Some(Choice::Ours), None], &[], (&o, &t));
        assert!(partial.starts_with("a\nours 1\nb\n<<<<<<< HEAD\nx\n"));
        assert!(partial.contains("||||||| base\nbase\n=======\ny\n>>>>>>> feature\n"));
    }

    #[test]
    fn unterminated_conflict_stays_text() {
        // One real conflict, then a stray start marker (a fixture, a doc about git…).
        let text =
            "a\n<<<<<<< HEAD\nx\n=======\ny\n>>>>>>> b\nc\n<<<<<<< not a conflict\nd\n=======\ne\n";
        let (segs, o, t) = parse(text);
        let conflicts = segs
            .iter()
            .filter(|s| matches!(s, Segment::Conflict { .. }))
            .count();
        assert_eq!(conflicts, 1);
        assert_eq!(
            render(&segs, &[Some(Choice::Ours)], &[], (&o, &t)),
            "a\nx\nc\n<<<<<<< not a conflict\nd\n=======\ne\n"
        );
        // Once the real one is chosen, nothing is left to resolve.
        let mut ed = ConflictEditor {
            path: "f".into(),
            choices: vec![None],
            recorded: vec![None],
            segments: segs,
            opened: text.into(),
            result: text.into(),
            manual: false,
            ours_label: o,
            theirs_label: t,
            rebase: false,
            solving: false,
            on_disk: 1,
            generation: 0,
            saving: false,
            original: original::State::default(),
            history: Default::default(),
            reused: None,
            memo: Default::default(),
        };
        assert_eq!(ed.unresolved(), 1);
        ed.choose(0, Choice::Ours);
        assert_eq!(ed.unresolved(), 0);
    }

    fn editor(text: &str) -> ConflictEditor {
        let (segments, ours_label, theirs_label) = parse(text);
        let conflicts = segments
            .iter()
            .filter(|s| matches!(s, Segment::Conflict { .. }))
            .count();
        ConflictEditor {
            path: "f".into(),
            choices: vec![None; conflicts],
            recorded: vec![None; conflicts],
            segments,
            opened: text.into(),
            result: text.into(),
            manual: false,
            ours_label,
            theirs_label,
            rebase: false,
            solving: false,
            on_disk: conflicts,
            generation: 0,
            saving: false,
            original: original::State::default(),
            history: Default::default(),
            reused: None,
            memo: Default::default(),
        }
    }

    /// Markers left by a hand edit keep the file from being saved as resolved, also when the
    /// edit removed a start marker the file had as text.
    #[test]
    fn hand_edits_that_leave_markers_are_unresolved() {
        let text = "a\n<<<<<<< HEAD\nx\n=======\ny\n>>>>>>> b\nc\n<<<<<<< not a conflict\nd\n";
        let mut ed = editor(text);
        assert_eq!(ed.unresolved(), 1);
        // The stray line deleted by hand, the real conflict still there.
        ed.manual = true;
        ed.result = text.replacen("<<<<<<< not a conflict\n", "", 1);
        assert_eq!(ed.unresolved(), 1);
        assert!(
            ed.save(Path::new("/nonexistent"))
                .is_err_and(|e| e.contains("unresolved"))
        );
        // A conflict taken apart: its end marker deleted, the rest kept.
        let real = "a\n<<<<<<< HEAD\nx\n=======\ny\n>>>>>>> b\nc\n";
        let mut ed = editor(real);
        ed.manual = true;
        ed.result = "a\n<<<<<<< HEAD\nx\n=======\ny\nc\n".into();
        assert_eq!(ed.unresolved(), 1);
        ed.result = "a\nx\n=======\ny\nc\n".into();
        assert_eq!(ed.unresolved(), 1);
        assert!(
            ed.save(Path::new("/nonexistent"))
                .is_err_and(|e| e.contains("unresolved"))
        );
        ed.result = "a\nxy\nc\n".into();
        assert_eq!(ed.unresolved(), 0);
        // A side that has a marker-like line as text (a heading's underline) can be chosen.
        let heading = "<<<<<<< HEAD\nx\n=======\nTitles\n=======\n>>>>>>> b\n";
        let mut ed = editor(heading);
        ed.choose(0, Choice::Theirs);
        assert_eq!(ed.result, "Titles\n=======\n");
        assert_eq!(ed.unresolved(), 0);
    }

    /// Choosing after editing the result by hand keeps the edits, as "Back to choices" says.
    #[test]
    fn choices_wait_while_edited_by_hand() {
        let mut ed = editor(FILE);
        ed.choose(0, Choice::Ours);
        ed.manual = true;
        ed.result.push_str("my edit\n");
        let edited = ed.result.clone();
        ed.choose(1, Choice::Theirs);
        assert_eq!(ed.result, edited);
        assert_eq!(ed.choices, vec![Some(Choice::Ours), Some(Choice::Theirs)]);
    }

    #[test]
    fn solved_elsewhere_leaves_the_rest_to_choose() -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!("gitcraft-solved-{}", std::process::id()));
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let two =
            "<<<<<<< HEAD\na\n=======\nb\n>>>>>>> x\nmid\n<<<<<<< HEAD\nc\n=======\nd\n>>>>>>> x\n";
        std::fs::write(dir.join("f"), two).map_err(|e| e.to_string())?;
        let mut ed = ConflictEditor::open(&dir, "f", None).ok_or("no conflicts")?;
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(ed.on_disk, 2);
        ed.choose(0, Choice::Ours);

        // One solved: the other is offered again, the earlier choice no longer applies.
        let one = "ab\nmid\n<<<<<<< HEAD\nc\n=======\nd\n>>>>>>> x\n";
        assert_eq!(ed.take_solved(one.into()), 1);
        assert_eq!(ed.choices, vec![None]);
        assert_eq!(ed.result, one);
        assert_eq!(ed.unresolved(), 1);
        ed.choose(0, Choice::Theirs);
        assert_eq!(ed.result, "ab\nmid\nd\n");

        // All solved: nothing left to choose, ready to save.
        assert_eq!(ed.take_solved("ab\nmid\ncd\n".into()), 0);
        assert_eq!(ed.unresolved(), 0);
        Ok(())
    }

    /// A merge leaves the other side's link where it conflicts with a file: the editor doesn't
    /// read (or later write) what it points to, which may never end (`/dev/zero`) or have
    /// conflict markers of its own.
    #[cfg(unix)]
    #[test]
    fn a_conflicted_link_is_not_followed() -> Result<(), String> {
        let dir =
            std::env::temp_dir().join(format!("gitcraft-conflict-link-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let marked = "<<<<<<< HEAD\na\n=======\nb\n>>>>>>> x\n";
        std::fs::write(dir.join("elsewhere"), marked).map_err(|e| e.to_string())?;
        std::os::unix::fs::symlink(dir.join("elsewhere"), dir.join("f"))
            .map_err(|e| e.to_string())?;
        std::os::unix::fs::symlink("/dev/zero", dir.join("z")).map_err(|e| e.to_string())?;
        let (link, zero) = (
            ConflictEditor::open(&dir, "f", None),
            ConflictEditor::open(&dir, "z", None),
        );
        let _ = std::fs::remove_dir_all(&dir);
        assert!(link.is_none() && zero.is_none());
        Ok(())
    }

    /// What was read or checked in the background for an editor closed since (or opened
    /// again since) is not taken for the one open: its choices stay. A change seen for the one
    /// open opens it again; not while it is being saved.
    #[test]
    fn results_for_an_earlier_editor_are_not_taken() -> Result<(), String> {
        use crate::git::{Area, StatusFile};
        use crate::repo::{DiffKey, RepoTab};
        let dir =
            std::env::temp_dir().join(format!("gitcraft-conflict-gen-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        std::fs::write(dir.join("f"), FILE).map_err(|e| e.to_string())?;
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
        let file = StatusFile {
            path: "f".to_owned(),
            old_path: None,
            code: 'U',
            area: Area::Conflicted,
        };
        tab.open_diff(DiffKey::Work(file), &ctx);
        let now = tab.conflict_generation;
        let chosen = |tab: &RepoTab| tab.conflict.as_ref().map(|ed| ed.choices.clone());
        if let Some(ed) = tab.conflict.as_mut() {
            assert_eq!(ed.generation, now);
            ed.choose(0, Choice::Ours);
        }
        let picked = Some(vec![Some(Choice::Ours), None]);

        // Read for an earlier opening: not taken.
        let earlier = ConflictEditor::open(&dir, "f", None).map(Box::new);
        tab.conflict_opened(now - 1, "f".to_owned(), earlier);
        assert_eq!(chosen(&tab), picked);
        // Changed, as seen for an earlier opening: nothing reopened.
        tab.conflict_changed(now - 1, "f".to_owned(), &ctx);
        assert_eq!(chosen(&tab), picked);
        // Nor while the one open is being saved.
        if let Some(ed) = tab.conflict.as_mut() {
            ed.saving = true;
        }
        tab.conflict_changed(now, "f".to_owned(), &ctx);
        assert_eq!(chosen(&tab), picked);
        if let Some(ed) = tab.conflict.as_mut() {
            ed.saving = false;
        }
        // Changed, as seen for the one open: opened again from the file.
        tab.conflict_changed(now, "f".to_owned(), &ctx);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(chosen(&tab), Some(vec![None, None]));
        assert_eq!(tab.conflict.as_ref().map(|ed| ed.generation), Some(now + 1));
        Ok(())
    }

    /// The save script on another machine replaces the file (keeping its mode) rather than
    /// write through a link, and refuses one that is a link.
    #[test]
    fn the_save_script_replaces_rather_than_follows() {
        let script = save_script("~/repo/dir/it's.txt");
        assert!(
            script.starts_with("f=\"$HOME\"'/repo/dir/it'\\''s.txt'\n"),
            "{script}"
        );
        assert!(script.contains("mktemp \"$HOME\"'/repo/dir'/.gitcraft-save.XXXXXX"));
        assert!(script.contains("if [ -L \"$f\" ] || [ ! -f \"$f\" ]; then exit 4; fi"));
        assert!(script.contains("mv -f -- \"$t\" \"$f\""));
    }

    /// Against WSL (`GITCRAFT_TEST_WSL=<distro>`; skipped otherwise): a solved file is saved
    /// with its mode, a link in its place is refused and left alone.
    #[cfg(windows)]
    #[test]
    fn a_solved_file_is_saved_there() -> Result<(), String> {
        let Ok(distro) = std::env::var("GITCRAFT_TEST_WSL") else {
            return Ok(());
        };
        let host = crate::remote::Host::Wsl(distro);
        let dir = format!("/tmp/gitcraft-conflict-save-{}", std::process::id());
        let sh = |script: String| {
            crate::remote::exec(
                &host,
                &crate::remote::Request {
                    script,
                    ..crate::remote::Request::default()
                },
            )
        };
        let made = sh(format!(
            "rm -rf {dir} && mkdir -p {dir} && printf 'x\\n' > {dir}/run.sh && chmod 755 \
             {dir}/run.sh && printf 'secret\\n' > {dir}/target && ln -s target {dir}/link"
        ))?;
        assert_eq!(made.code, 0, "{}", String::from_utf8_lossy(&made.stderr));
        let repo = crate::remote::encode(&host, &dir);
        let saved = save_solved(&repo, "run.sh", "solved\n");
        let linked = save_solved(&repo, "link", "over the link\n");
        let after = sh(format!(
            "cat {dir}/run.sh; stat -c %a {dir}/run.sh; cat {dir}/target; [ -L {dir}/link ] && \
             echo link; ls -a {dir} | grep -c gitcraft-save"
        ))?;
        let _ = sh(format!("rm -rf {dir}"));
        assert_eq!(saved, Ok(()));
        assert!(linked.is_err(), "{linked:?}");
        assert_eq!(
            String::from_utf8_lossy(&after.stdout),
            "solved\n755\nsecret\nlink\n0\n"
        );
        Ok(())
    }

    #[test]
    fn longer_runs_are_content() {
        // A Markdown heading underline and nested (longer) markers don't split the conflict.
        let text = "<<<<<<< HEAD\nTitle\n========\n<<<<<<<< inner\n=======\ny\n>>>>>>> b\n";
        let (segs, o, t) = parse(text);
        assert_eq!(
            segs,
            vec![Segment::Conflict {
                ours: "Title\n========\n<<<<<<<< inner\n".into(),
                base: None,
                theirs: "y\n".into(),
            }]
        );
        assert_eq!(render(&segs, &[Some(Choice::Theirs)], &[], (&o, &t)), "y\n");
        assert!(is_marker("<<<<<<<", '<'));
        assert!(is_marker(">>>>>>> feature", '>'));
        assert!(!is_marker("<<<<<<<x", '<'));
        assert!(!is_marker("======", '='));
    }

    #[test]
    fn keeps_crlf_line_endings() {
        let crlf = "a\r\n<<<<<<< HEAD\r\nmine\r\n=======\r\nyours\r\n>>>>>>> b\r\nz\r\n";
        let (segs, o, t) = parse(crlf);
        assert_eq!(
            render(&segs, &[Some(Choice::Ours)], &[], (&o, &t)),
            "a\r\nmine\r\nz\r\n"
        );
    }
}
