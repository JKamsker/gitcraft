//! Bounded committed-line history. No working file, index, filter or forge is read here.
//! A card is a real pinned file version; attribution and uncertain parent mappings are separate.
use super::*;
use std::collections::{BTreeSet, HashMap};
const FILE: usize = 2 * 1024 * 1024;
const TOTAL: usize = 32 * 1024 * 1024;
const LINES: usize = 20000;
const STEPS: usize = 128;
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct Seed {
    pub commit: String,
    pub path: String,
    pub line: usize,
}
#[derive(Clone, Debug)]
pub(crate) struct Attribution {
    pub commit: String,
    pub path: String,
    pub line: usize,
}
#[derive(Clone, Debug)]
pub(crate) struct Card {
    pub at: Seed,
    pub text: String,
    pub file_oid: String,
    pub author: String,
    pub email: String,
    pub time: i64,
    pub subject: String,
    pub attribution: Attribution,
    pub note: String,
    pub whitespace_only: bool,
}
#[derive(Clone, Debug)]
pub(crate) struct Dossier {
    pub start: Seed,
    pub cards: Vec<Card>,
    pub stopped: String,
    pub continuation: Option<Seed>,
    pub alternatives: Vec<Seed>,
}

/// `revision` is already a graph/file snapshot ID, or HEAD for a deliberate committed view.
pub(crate) fn trace(
    repo: &Path,
    revision: &str,
    path: &str,
    line: usize,
    active: &dyn Fn() -> bool,
) -> Result<Dossier, String> {
    if !(oid(revision) || revision == "HEAD") || !path_ok(path) || !(1..=LINES).contains(&line) {
        return Err(crate::i18n::text(
            "backend-choose-a-valid-committed-file-and-line-up-to20000-lines-ff8536",
        )
        .into());
    }
    let mut t = Trace {
        reader: Reader::new(repo, active),
        files: HashMap::new(),
        blobs: HashMap::new(),
        bytes: 0,
    };
    let resolved = t.reader.run(
        &[
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{revision}^{{commit}}"),
        ],
        vec![],
        &[0],
    )?;
    let commit = std::str::from_utf8(&resolved)
        .map_err(|_| crate::i18n::text("backend-invalid-pinned-commit-b28d61"))?
        .trim()
        .to_owned();
    if !oid(&commit) {
        return Err(crate::i18n::text("backend-invalid-pinned-history-commit-6a5b76").into());
    }
    let start = Seed {
        commit,
        path: path.into(),
        line,
    };
    let mut cursor = start.clone();
    let mut result = Dossier {
        start,
        cards: Vec::new(),
        stopped: String::new(),
        continuation: None,
        alternatives: vec![],
    };
    let mut seen = BTreeSet::new();
    for step in 0..STEPS {
        if !t.reader.active() {
            return Err("Line history cancelled or exceeded120seconds".into());
        }
        if !seen.insert(cursor.clone()) {
            result.stopped = crate::i18n::text(
                "backend-history-mapping-returned-to-a-previously-visited-line-13b05d",
            )
            .into();
            break;
        }
        let file = t
            .file(&cursor.commit, &cursor.path)?
            .ok_or(crate::i18n::text(
                "backend-pinned-history-file-is-missing-7e85cd",
            ))?;
        let text = one_line(&file.text, cursor.line)?.to_owned();
        let range = format!("{},{}", cursor.line, cursor.line);
        let attribution = blame_frame(&t.reader.run(
            &[
                "-c",
                "i18n.logOutputEncoding=UTF-8",
                "blame",
                "-w",
                "-C",
                "-C",
                "-M",
                "--line-porcelain",
                "--no-textconv",
                "--no-ignore-revs-file",
                "-L",
                &range,
                &cursor.commit,
                "--",
                &cursor.path,
            ],
            vec![],
            &[0],
        )?)?;
        let origin = t
            .file(&attribution.commit, &attribution.path)?
            .ok_or(crate::i18n::text(
                "backend-the-attributed-file-is-missing-5beb1a",
            ))?;
        let original = one_line(&origin.text, attribution.line)?;
        if normal(original) != normal(&text) {
            return Err(crate::i18n::text(
                "backend-blame-attribution-did-not-match-the-pinned-raw-line-93aee5",
            )
            .into());
        }
        let (parents, author, email, time, subject) = t.parents(&cursor.commit)?;
        let mut mapped = Vec::new();
        for parent in &parents {
            if let Some(path) = t.old_path(parent, &cursor.commit, &cursor.path)?
                && let Some(old) = t.file(parent, &path)?
                && let Some((line, note)) = previous_line(&file.text, &old.text, cursor.line)?
            {
                mapped.push((
                    Seed {
                        commit: parent.clone(),
                        path,
                        line,
                    },
                    old,
                    note,
                ));
            }
        }
        // A partial copy may be below whole-file rename/copy similarity. Follow its
        // precise -C origin only after proving it belongs to an actual parent history.
        let mut copied = None;
        if mapped.is_empty() && attribution.commit != cursor.commit {
            for parent in &parents {
                let base = t.reader.run(
                    &["merge-base", &attribution.commit, parent],
                    vec![],
                    &[0, 1],
                )?;
                if base.trim_ascii() == attribution.commit.as_bytes() {
                    copied = Some(Seed {
                        commit: attribution.commit.clone(),
                        path: attribution.path.clone(),
                        line: attribution.line,
                    });
                    break;
                }
            }
        }
        let changed = mapped.first().is_none_or(|(seed, old, _)| {
            seed.path != cursor.path || one_line(&old.text, seed.line).ok() != Some(text.as_str())
        });
        if step == 0 || changed {
            let whitespace_only = parents.len() == 1
                && changed
                && mapped.first().is_some_and(|(seed, old, _)| {
                    seed.path == cursor.path
                        && normal(&old.text) == normal(&file.text)
                        && old.text != file.text
                });
            result.cards.push(Card {
                at: cursor.clone(),
                text,
                file_oid:file.oid.clone(),
                author,
                email,
                time,
                subject,
                attribution,
                note: if copied.is_some(){
                    crate::i18n::text("backend-git-copy-move-attribution-verified-in-a-parent-history-whole-file-36df4f").into()
                } else if step == 0 && !changed {
                    crate::i18n::text("backend-selected-snapshot-context-attribution-names-the-last-semantic-edi-cb4480").into()
                } else {
                    mapped
                        .first()
                        .map_or(crate::i18n::text("backend-no-unique-parent-line-mapping-cb23e8"), |(_, _, note)| *note)
                        .into()
                },
                whitespace_only,
            });
        }
        if parents.is_empty() {
            result.stopped =
                crate::i18n::text("backend-reached-a-root-or-shallow-history-boundary-50fce1")
                    .into();
            break;
        }
        if let Some(next) = copied {
            cursor = next;
            if step + 1 == STEPS {
                result.stopped =
                    crate::i18n::text("backend-128-history-steps-reached-continue-from-the-pinned-earlier-line-d9e1e1").into();
                result.continuation = Some(cursor.clone());
            }
            continue;
        }
        if mapped.is_empty() {
            result.stopped = crate::i18n::text(
                "backend-line-introduction-or-an-ambiguous-unequal-parent-edit-no-earlier-ab2444",
            )
            .into();
            break;
        }
        if parents.len() > 1 {
            result.alternatives = mapped.into_iter().map(|(seed, _, _)| seed).collect();
            result.stopped = crate::i18n::text(
                "backend-merge-boundary-choose-an-explicitly-mapped-parent-to-continue-oth-3a0c2f",
            )
            .into();
            break;
        }
        let (mut next, old, _) = mapped.remove(0);
        if let Some(touch) = t.touch(&next.commit, &next.path)?
            && touch != next.commit
            && let Some(previous) = t.file(&touch, &next.path)?
            && previous.oid == old.oid
        {
            next.commit = touch;
        }
        cursor = next;
        if step + 1 == STEPS {
            result.stopped = crate::i18n::text(
                "backend-128-history-steps-reached-continue-from-the-pinned-earlier-line-d9e1e1",
            )
            .into();
            result.continuation = Some(cursor.clone());
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests;

mod history;
use history::Trace;
use history::blame_frame;
use history::normal;
use history::one_line;
use history::path_ok;
use history::previous_line;
pub(crate) use history::quoted_path;
