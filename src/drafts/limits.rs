//! How much is kept: drafts are for what would hurt to lose, not for whole files pasted into a
//! box. Each text is kept up to a size, the lists up to a length, and the file up to a size.

use super::{RebaseDraft, RepoDrafts};

/// The longest text kept of one box, in bytes.
pub(super) const MAX_TEXT: usize = 256 * 1024;
/// The most pull requests whose drafts are kept for one repository.
pub(super) const MAX_PULL_REQUESTS: usize = 32;
/// The most commits of a rebase list kept.
pub(super) const MAX_REBASE_STEPS: usize = 2_000;
/// The largest file written, or read back.
pub(super) const MAX_FILE: usize = 4 * 1024 * 1024;

/// `drafts` cut down to the limits, and whether anything had to be cut.
pub(super) fn bounded(mut drafts: RepoDrafts) -> (RepoDrafts, bool) {
    let mut cut = false;
    let commit = &mut drafts.commit;
    cut |= clip(&mut commit.summary) | clip(&mut commit.body);
    // The pull requests with the highest numbers (the newest) are the ones kept.
    while drafts.pull_requests.len() > MAX_PULL_REQUESTS {
        drafts.pull_requests.pop_first();
        cut = true;
    }
    for text in drafts.pull_requests.values_mut() {
        cut |= clip(&mut text.comment) | clip(&mut text.review) | clip(&mut text.line_text);
    }
    if let Some(rebase) = &mut drafts.rebase {
        cut |= bound_rebase(rebase);
    }
    (drafts, cut)
}

/// A rebase list too long to keep is not kept at all: part of one would rebase other commits.
fn bound_rebase(rebase: &mut RebaseDraft) -> bool {
    let mut cut = false;
    for entry in &mut rebase.entries {
        cut |= clip(&mut entry.message) | clip(&mut entry.original);
    }
    cut
}

/// The file for `drafts`, within [`MAX_FILE`]: what doesn't fit goes, the rebase list first,
/// then the pull requests' drafts with the most text. `Err` when even the commit message alone
/// doesn't fit (it can't, once bounded), or the drafts can't be written out.
pub(super) fn encode(
    repo: &std::path::Path,
    drafts: RepoDrafts,
) -> Result<(Vec<u8>, bool), String> {
    let (mut drafts, mut cut) = bounded(drafts);
    if drafts
        .rebase
        .as_ref()
        .is_some_and(|r| r.entries.len() > MAX_REBASE_STEPS)
    {
        drafts.rebase = None;
        cut = true;
    }
    loop {
        let file = super::DraftFile {
            format: super::FORMAT,
            repo: repo.to_owned(),
            drafts,
        };
        let bytes = serde_json::to_vec(&file).map_err(|e| e.to_string())?;
        if bytes.len() <= MAX_FILE {
            return Ok((bytes, cut));
        }
        drafts = file.drafts;
        cut = true;
        if drafts.rebase.take().is_some() {
            continue;
        }
        let largest = drafts
            .pull_requests
            .iter()
            .max_by_key(|(_, t)| t.comment.len() + t.review.len() + t.line_text.len())
            .map(|(number, _)| *number);
        match largest {
            Some(number) => {
                drafts.pull_requests.remove(&number);
            }
            None => return Err("the commit message is too large to keep".to_owned()),
        }
    }
}

/// Cuts `text` to [`MAX_TEXT`] bytes, at a character's boundary. Whether it was cut.
fn clip(text: &mut String) -> bool {
    if text.len() <= MAX_TEXT {
        return false;
    }
    let mut end = MAX_TEXT;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text.truncate(end);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drafts::{PrText, RebaseStep};
    use std::path::Path;

    #[test]
    fn text_is_cut_on_a_character_boundary() {
        // 'é' is two bytes: one more than the limit can only be cut before it.
        let mut text = "a".repeat(MAX_TEXT - 1) + "é";
        assert!(clip(&mut text));
        assert_eq!(text.len(), MAX_TEXT - 1);
        let mut short = "Fix ünïcode 🦀".to_owned();
        assert!(!clip(&mut short));
        assert_eq!(short, "Fix ünïcode 🦀");
    }

    #[test]
    fn the_newest_pull_requests_are_kept() {
        let mut drafts = RepoDrafts::default();
        for n in 1..=(MAX_PULL_REQUESTS as u64 + 5) {
            drafts.pull_requests.insert(
                n,
                PrText {
                    comment: format!("on {n}"),
                    ..PrText::default()
                },
            );
        }
        let (drafts, cut) = bounded(drafts);
        assert!(cut);
        assert_eq!(drafts.pull_requests.len(), MAX_PULL_REQUESTS);
        assert!(
            drafts
                .pull_requests
                .contains_key(&(MAX_PULL_REQUESTS as u64 + 5))
        );
        assert!(!drafts.pull_requests.contains_key(&1));
    }

    #[test]
    fn a_file_too_large_drops_the_rebase_list_then_the_largest_comment() {
        let big = "x".repeat(MAX_TEXT);
        let mut drafts = RepoDrafts::default();
        drafts.commit.summary = "Keep me".to_owned();
        for n in 0..MAX_PULL_REQUESTS as u64 {
            drafts.pull_requests.insert(
                n,
                PrText {
                    comment: big.clone(),
                    review: big.clone(),
                    ..PrText::default()
                },
            );
        }
        let step = RebaseStep {
            hash: "a".repeat(40),
            author: String::new(),
            email: String::new(),
            summary: String::new(),
            original: String::new(),
            merge: false,
            step: "pick".to_owned(),
            message: String::new(),
        };
        drafts.rebase = Some(RebaseDraft {
            onto: "main".to_owned(),
            onto_label: "main".to_owned(),
            head: String::new(),
            entries: vec![step],
        });
        let (bytes, cut) = encode(Path::new("/r"), drafts).unwrap_or_default();
        assert!(cut);
        assert!(bytes.len() <= MAX_FILE);
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("Keep me"));
        assert!(text.contains("\"rebase\":null"));
    }

    #[test]
    fn a_rebase_list_too_long_is_not_kept_in_part() {
        let mut drafts = RepoDrafts::default();
        let step = RebaseStep {
            hash: "a".to_owned(),
            author: String::new(),
            email: String::new(),
            summary: String::new(),
            original: String::new(),
            merge: false,
            step: "pick".to_owned(),
            message: String::new(),
        };
        drafts.rebase = Some(RebaseDraft {
            onto: "main".to_owned(),
            onto_label: "main".to_owned(),
            head: String::new(),
            entries: vec![step; MAX_REBASE_STEPS + 1],
        });
        let (bytes, cut) = encode(Path::new("/r"), drafts).unwrap_or_default();
        assert!(cut);
        assert!(String::from_utf8_lossy(&bytes).contains("\"rebase\":null"));
    }
}
