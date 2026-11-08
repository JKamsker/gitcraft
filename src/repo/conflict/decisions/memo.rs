//! What the conflict editor holds of decisions: those stored for its conflicts (offered, or
//! applied when the code around them is unchanged), and the note and test being written for
//! each conflict.

use super::record::{self, Decision, Spot};
use crate::repo::conflict::{Choice, ConflictEditor, split_resolutions};

/// A stored decision for one conflict of the file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Offer {
    pub decision: Decision,
    /// The code around the conflict changed since it was made.
    pub stale: bool,
    /// Show the code around it as it was and as it is.
    pub show_context: bool,
    /// The lines before and after the conflict now.
    pub now: (String, String),
    /// Show the end of what its test printed the last time.
    pub show_output: bool,
}

/// The note and test the user wrote for one conflict (never filled in from a decision read:
/// a fetched one's test is only run once the user takes it as theirs).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Draft {
    /// The fields are shown.
    pub open: bool,
    pub note: String,
    pub test: String,
}

impl Draft {
    /// Something to keep: a note or a test.
    pub fn is_empty(&self) -> bool {
        self.note.trim().is_empty() && self.test.trim().is_empty()
    }
}

/// The decisions side of an open conflict editor.
#[derive(Debug, Default)]
pub struct Memo {
    /// Each conflict, as decisions are matched against it.
    pub spots: Vec<Spot>,
    /// The decision stored for each conflict, once read (`None` until then).
    pub offers: Option<Vec<Option<Offer>>>,
    /// Being read.
    pub reading: bool,
    pub drafts: Vec<Draft>,
    /// Run the tests written or used once the file is saved.
    pub run_tests: bool,
}

/// What is to be stored once the file is saved: each decision with its key.
pub type ToRecord = Vec<(String, Decision)>;

/// One test command (a line: git runs it as an alias), its spaces at the ends trimmed.
pub fn one_line(command: &str) -> String {
    // A line continuation joins its lines, as the shell would.
    command
        .replace("\\\r\n", " ")
        .replace("\\\n", " ")
        .split(['\n', '\r'])
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("; ")
}

impl ConflictEditor {
    /// The conflicts as decisions are matched against them, read afresh from the editor.
    pub(in crate::repo) fn decision_spots(&self) -> Vec<Spot> {
        record::spots(&self.segments)
    }

    /// Takes the decisions read for the conflicts (`found`, one per conflict): one made with
    /// the code around it as it is now is applied to an undecided conflict, the others are
    /// offered. Returns how many were applied.
    pub(in crate::repo) fn take_offers(&mut self, found: Vec<Option<Decision>>) -> usize {
        let spots = self.memo.spots.clone();
        let mut offers = Vec::with_capacity(spots.len());
        let mut applied = 0;
        self.memo.drafts.resize_with(spots.len(), Draft::default);
        for (k, (spot, decision)) in spots.iter().zip(found).enumerate() {
            let Some(decision) = decision.filter(|d| record::is_for(d, spot)) else {
                offers.push(None);
                continue;
            };
            let stale = !record::is_fresh(&decision, spot);
            let undecided = self.choices.get(k).copied().flatten().is_none();
            if !stale && undecided && !self.manual && self.use_text(k, &decision.resolution) {
                applied += 1;
            }
            offers.push(Some(Offer {
                decision,
                stale,
                show_context: false,
                now: (spot.before.clone(), spot.after.clone()),
                show_output: false,
            }));
        }
        self.memo.offers = Some(offers);
        self.memo.reading = false;
        applied
    }

    /// Resolves conflict `k` with `text` (a decision's resolution), as a recorded choice, its
    /// line ends as the conflict's own (a decision made where the file had LF, used where it
    /// has CRLF).
    fn use_text(&mut self, k: usize, text: &str) -> bool {
        let crlf = self
            .memo
            .spots
            .get(k)
            .is_some_and(|s| s.ours.contains("\r\n") || s.theirs.contains("\r\n"));
        let lf = text.replace("\r\n", "\n");
        let text = if crlf { lf.replace('\n', "\r\n") } else { lf };
        let Some(slot) = self.recorded.get_mut(k) else {
            return false;
        };
        *slot = Some(text);
        if let Some(choice) = self.choices.get_mut(k) {
            *choice = Some(Choice::Recorded);
        }
        if !self.manual {
            self.result = self.rendered();
        }
        true
    }

    /// Takes the note and test of the decision offered for conflict `k` as the user's own: kept
    /// with this resolution when saved, its test run when asked.
    pub(in crate::repo) fn take_note(&mut self, k: usize) {
        let Some((note, test)) = self
            .offer(k)
            .map(|o| (o.decision.note.clone(), o.decision.test.clone()))
        else {
            return;
        };
        if let Some(draft) = self.memo.drafts.get_mut(k) {
            draft.note = note;
            draft.test = test;
            draft.open = true;
        }
    }

    /// Keeps the notes written for the conflicts that are still there once others were solved
    /// (by Mergiraf): `before`, the conflicts as they were.
    pub(in crate::repo) fn carry_drafts(&mut self, before: &[Spot], memo: Memo) {
        let now = self.decision_spots();
        self.memo.drafts = now
            .iter()
            .map(|spot| {
                before
                    .iter()
                    .position(|b| b.key == spot.key)
                    .and_then(|k| memo.drafts.get(k).cloned())
                    .unwrap_or_default()
            })
            .collect();
        self.memo.run_tests = memo.run_tests;
    }

    /// Uses the decision offered for conflict `k` (also a stale one: asked for).
    pub(in crate::repo) fn use_offer(&mut self, k: usize) {
        let Some(text) = self
            .memo
            .offers
            .as_ref()
            .and_then(|o| o.get(k))
            .and_then(Option::as_ref)
            .map(|o| o.decision.resolution.clone())
        else {
            return;
        };
        self.use_text(k, &text);
    }

    /// The text that replaces each conflict in the result: from the choices, or, edited by
    /// hand, found again in it; `None` when it can't be told apart.
    fn resolutions(&self) -> Option<Vec<String>> {
        if self.manual {
            return split_resolutions(&self.segments, &self.result);
        }
        let mut texts = Vec::new();
        let mut k = 0;
        for seg in &self.segments {
            let crate::repo::conflict::Segment::Conflict { ours, theirs, .. } = seg else {
                continue;
            };
            let text = match self.choices.get(k).copied().flatten()? {
                Choice::Ours => ours.clone(),
                Choice::Theirs => theirs.clone(),
                Choice::OursThenTheirs => format!("{ours}{theirs}"),
                Choice::TheirsThenOurs => format!("{theirs}{ours}"),
                Choice::Recorded => self.recorded.get(k)?.clone()?,
            };
            texts.push(text);
            k += 1;
        }
        Some(texts)
    }

    /// The decisions to store as the file is saved (`by`: who resolved it, `at`: when):
    /// - each conflict with a note or a test the user wrote, with how it was resolved;
    /// - a stale decision used anyway: kept as it was made (its author, note, test), the code
    ///   around it as it is now, its test not run on it yet.
    ///
    /// An error says why none can be: the result was edited by hand around its conflicts.
    pub(in crate::repo) fn to_record(&self, by: &str, at: i64) -> Result<ToRecord, String> {
        let wanted: Vec<usize> = (0..self.choices.len())
            .filter(|&k| {
                self.memo.drafts.get(k).is_some_and(|d| !d.is_empty())
                    || self.offer(k).is_some_and(|o| o.stale)
            })
            .collect();
        if wanted.is_empty() {
            return Ok(Vec::new());
        }
        let texts = self.resolutions().ok_or_else(|| {
            crate::i18n::text("repo-core-0476")
                .to_owned()
        })?;
        let spots = record::spots(&self.segments);
        let same = |a: &str, b: &str| a.replace("\r\n", "\n") == b.replace("\r\n", "\n");
        let mut out = Vec::new();
        for k in wanted {
            let (Some(spot), Some(text)) = (spots.get(k), texts.get(k)) else {
                continue;
            };
            let offer = self.offer(k);
            let draft = self.memo.drafts.get(k).filter(|d| !d.is_empty());
            let decision = match (draft, offer) {
                (Some(draft), _) => {
                    // How its test last went, when it ran on this very resolution and code.
                    let last_test = offer
                        .filter(|o| {
                            !o.stale
                                && o.decision.test == one_line(&draft.test)
                                && same(&o.decision.resolution, text)
                        })
                        .and_then(|o| o.decision.last_test.clone());
                    Decision {
                        path: self.path.clone(),
                        note: draft.note.trim().to_owned(),
                        test: one_line(&draft.test),
                        resolution: text.clone(),
                        ours: spot.ours.clone(),
                        theirs: spot.theirs.clone(),
                        before: spot.before.clone(),
                        after: spot.after.clone(),
                        by: by.to_owned(),
                        at,
                        last_test,
                    }
                }
                // A stale one, used anyway: brought up to date. Not used: left as it is.
                (None, Some(o)) if same(&o.decision.resolution, text) => Decision {
                    before: spot.before.clone(),
                    after: spot.after.clone(),
                    last_test: None,
                    ..o.decision.clone()
                },
                _ => continue,
            };
            out.push((spot.key.clone(), decision));
        }
        Ok(out)
    }

    /// The decision offered for conflict `k`.
    pub(in crate::repo) fn offer(&self, k: usize) -> Option<&Offer> {
        self.memo.offers.as_ref()?.get(k)?.as_ref()
    }

    /// Whether any conflict has a test the user wrote, to run once saved.
    pub(in crate::repo) fn has_tests(&self) -> bool {
        self.memo.drafts.iter().any(|d| !d.test.trim().is_empty())
    }
}
