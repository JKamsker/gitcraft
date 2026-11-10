use super::*;

/// How a label beside a menu entry is coloured.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::repo) enum Tone {
    Faint,
    Good,
    Warn,
}

/// The few words beside the menu entry of `op`, if any.
pub(in crate::repo) fn label(op: Op, check: &Check) -> Option<(String, Tone)> {
    match check {
        Check::Checking => Some((crate::i18n::text("repo-core-0765").to_owned(), Tone::Faint)),
        Check::Unavailable(_) => None,
        Check::Failed(_) => Some((crate::i18n::text("repo-core-0766").to_owned(), Tone::Faint)),
        Check::Done(verdict) => Some(match &verdict.prediction.outcome {
            Outcome::Clean { changes: false } if op == Op::CherryPick => {
                (crate::i18n::text("repo-core-0767").to_owned(), Tone::Faint)
            }
            Outcome::Clean { changes: false } if op == Op::Merge => {
                (crate::i18n::text("repo-core-0768").to_owned(), Tone::Faint)
            }
            Outcome::Clean { .. } => (crate::i18n::text("repo-core-0769").to_owned(), Tone::Good),
            Outcome::Conflicts { files, .. } => (
                crate::i18n::message(
                    "repo-core-0770",
                    &[("len", files_count(files.len()).to_string().into())],
                ),
                Tone::Warn,
            ),
            Outcome::Unsure(_) => (crate::i18n::text("repo-core-0771").to_owned(), Tone::Warn),
        }),
    }
}

/// `1 file`, `3 files`.
pub(super) fn files_count(n: usize) -> String {
    if n == 1 {
        crate::i18n::text("repo-core-0772").to_owned()
    } else {
        crate::i18n::message("repo-core-0773", &[("n", (n).into())])
    }
}

/// How many conflicted files a tooltip lists; the rest are counted.
pub(super) const LISTED: usize = 15;

/// What hovering a checked entry says: the outcome, the conflicted files with what Mergiraf
/// could do with each, and that uncommitted changes (`dirty`) are left out.
pub(in crate::repo) fn tooltip(op: Op, check: &Check, dirty: bool) -> String {
    let mut text = match check {
        Check::Checking => crate::i18n::text("repo-core-0774").to_owned(),
        Check::Unavailable(why) => return why.clone(),
        Check::Failed(e) => crate::i18n::message("repo-core-0775", &[("e", e.to_string().into())]),
        Check::Done(verdict) => verdict_text(op, verdict),
    };
    if dirty && !matches!(check, Check::Unavailable(_)) {
        text.push_str(crate::i18n::text("repo-core-0776"));
    }
    text
}

pub(super) fn verdict_text(op: Op, verdict: &Verdict) -> String {
    let commits = verdict.prediction.commits;
    match &verdict.prediction.outcome {
        Outcome::Clean { changes: false } if op == Op::CherryPick => {
            crate::i18n::text("repo-core-0777").to_owned()
        }
        Outcome::Clean { changes: false } if op == Op::Merge => {
            crate::i18n::text("repo-core-0778").to_owned()
        }
        Outcome::Clean { .. } if op == Op::Rebase && commits > 0 => {
            let all = if commits == 1 {
                crate::i18n::text("repo-core-0779").to_owned()
            } else {
                crate::i18n::message("repo-core-0780", &[("commits", (commits).into())])
            };
            crate::i18n::message("repo-core-0781", &[("all", all.to_string().into())])
        }
        Outcome::Clean { .. } => crate::i18n::text("repo-core-0782").to_owned(),
        Outcome::Unsure(why) => {
            crate::i18n::message("repo-core-0783", &[("why", why.to_string().into())])
        }
        Outcome::Conflicts { files, stop } => {
            let mut text = match stop {
                Some(stop) if op == Op::Rebase || commits > 1 => crate::i18n::message(
                    "repo-core-0784",
                    &[
                        ("index", format!("{}", stop.index).into()),
                        ("commits", format!("{}", commits).into()),
                        (
                            "hash",
                            crate::repo::short(&stop.commit.hash).to_string().into(),
                        ),
                        ("summary", stop.commit.summary.to_string().into()),
                        ("len", files_count(files.len()).to_string().into()),
                    ],
                ),
                _ => crate::i18n::message(
                    "repo-core-0785",
                    &[("len", files_count(files.len()).to_string().into())],
                ),
            };
            text.push_str(&file_lines(files, &verdict.mergiraf));
            if let Some(stop) = stop
                && stop.index < commits
            {
                text.push_str(crate::i18n::text("repo-core-0786"));
            }
            let unchecked = verdict.mergiraf.iter().find_map(|m| match m {
                Mergiraf::Unchecked(e) => Some(e),
                _ => None,
            });
            if let Some(e) = unchecked {
                text.push_str(&crate::i18n::message(
                    "repo-core-0787",
                    &[("e", e.to_string().into())],
                ));
            }
            if verdict.mergiraf.contains(&Mergiraf::NotInstalled) {
                text.push_str(crate::i18n::text("repo-core-0788"));
            }
            text
        }
    }
}

/// One line for each conflicted file (up to [`LISTED`]), with what Mergiraf could do.
pub(super) fn file_lines(files: &[ConflictFile], mergiraf: &[Mergiraf]) -> String {
    let mut text = String::new();
    for (n, file) in files.iter().enumerate().take(LISTED) {
        let hint = match mergiraf.get(n) {
            Some(Mergiraf::Likely) => crate::i18n::text("repo-core-0789").to_owned(),
            Some(Mergiraf::UnknownLanguage) => crate::i18n::text("repo-core-0790").to_owned(),
            Some(Mergiraf::NotText(kinds)) => {
                crate::i18n::message("repo-core-0791", &[("kinds", kinds.to_string().into())])
            }
            Some(Mergiraf::NotInstalled | Mergiraf::Unchecked(_)) | None => {
                crate::i18n::text("repo-core-0792").to_owned()
            }
        };
        text.push_str(&format!("\n• {} — {hint}", file.path));
    }
    if files.len() > LISTED {
        text.push_str(&crate::i18n::message(
            "repo-core-0794",
            &[("LISTED", format!("{}", files.len() - LISTED).into())],
        ));
    }
    text
}

/// How long the toolbar's target has to stay the same before it is checked: refs move a few
/// times in a row during a fetch, a pull or a checkout.
pub(super) const SETTLE: Duration = Duration::from_millis(800);
