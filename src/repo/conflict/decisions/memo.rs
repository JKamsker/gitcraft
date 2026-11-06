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
}

/// The note and test written for one conflict.
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
    command
        .split(['\n', '\r'])
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

impl ConflictEditor {
    /// The conflicts as decisions are matched against them, read afresh from the editor.
    pub(in crate::repo) fn decision_spots(&self) -> Vec<Spot> {
        record::spots(&self.segments)
    }

    /// Takes the decisions read for the conflicts (`found`, one per conflict): one made with
    /// the code around it as it is now is applied to an undecided conflict, the others are
    /// offered. Each fills that conflict's note and test. Returns how many were applied.
    pub(in crate::repo) fn take_offers(&mut self, found: Vec<Option<Decision>>) -> usize {
        let spots = std::mem::take(&mut self.memo.spots);
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
            if let Some(draft) = self.memo.drafts.get_mut(k).filter(|d| d.is_empty()) {
                draft.note.clone_from(&decision.note);
                draft.test.clone_from(&decision.test);
            }
            offers.push(Some(Offer {
                decision,
                stale,
                show_context: false,
                now: (spot.before.clone(), spot.after.clone()),
            }));
        }
        self.memo.spots = spots;
        self.memo.offers = Some(offers);
        self.memo.reading = false;
        applied
    }

    /// Resolves conflict `k` with `text` (a decision's resolution), as a recorded choice.
    fn use_text(&mut self, k: usize, text: &str) -> bool {
        let Some(slot) = self.recorded.get_mut(k) else {
            return false;
        };
        *slot = Some(text.to_owned());
        if let Some(choice) = self.choices.get_mut(k) {
            *choice = Some(Choice::Recorded);
        }
        if !self.manual {
            self.result = self.rendered();
        }
        true
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

    /// The decisions to store as the file is saved: each conflict with a note or a test, with
    /// how it was resolved (`by`: who resolved it, `at`: when). An error says why none can be:
    /// the result was edited by hand around its conflicts.
    pub(in crate::repo) fn to_record(&self, by: &str, at: i64) -> Result<ToRecord, String> {
        let wanted: Vec<usize> = (self.memo.drafts.iter().enumerate())
            .filter(|(_, d)| !d.is_empty())
            .map(|(k, _)| k)
            .collect();
        if wanted.is_empty() {
            return Ok(Vec::new());
        }
        let texts = self.resolutions().ok_or_else(|| {
            "The notes were not kept: the text around the conflicts was edited by hand, so \
             which part resolves which conflict can't be told"
                .to_owned()
        })?;
        let spots = record::spots(&self.segments);
        let mut out = Vec::new();
        for k in wanted {
            let (Some(spot), Some(text), Some(draft)) =
                (spots.get(k), texts.get(k), self.memo.drafts.get(k))
            else {
                continue;
            };
            // Kept from what was stored: how its test last went, when unchanged.
            let last_test = self
                .offer(k)
                .filter(|o| o.decision.test == one_line(&draft.test))
                .and_then(|o| o.decision.last_test.clone());
            out.push((
                spot.key.clone(),
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
                },
            ));
        }
        Ok(out)
    }

    /// The decision offered for conflict `k`.
    pub(in crate::repo) fn offer(&self, k: usize) -> Option<&Offer> {
        self.memo.offers.as_ref()?.get(k)?.as_ref()
    }

    /// Whether any conflict has a test to run once saved.
    pub(in crate::repo) fn has_tests(&self) -> bool {
        self.memo.drafts.iter().any(|d| !d.test.trim().is_empty())
    }
}
