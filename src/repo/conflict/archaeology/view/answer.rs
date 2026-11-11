use super::*;

pub(super) fn search_answer(
    ui: &mut Ui,
    t: &Tokens,
    history: &History,
    timelines: &Timelines,
    names: (&str, &str),
) {
    let Some(search) = &history.search else {
        return;
    };
    match &search.result {
        Read::NotAsked | Read::Reading => {}
        Read::Failed(e) => {
            ui.label(RichText::new(e).color(t.danger));
        }
        Read::Done(found) => {
            ui.horizontal_wrapped(|ui| {
                crate::icons::inline(ui, "search", 13.0, t.accent);
                ui.label(RichText::new(answer_text(found, timelines, names)).color(t.text));
            });
        }
    }
}

/// A commit as the answer names it.
pub(super) fn named(step: &Step) -> String {
    if step.summary.is_empty() {
        step.short.clone()
    } else {
        format!("{} “{}”", step.short, step.summary)
    }
}

/// The answer in words (`names`: the two sides').
pub(super) fn answer_text(found: &Found, timelines: &Timelines, names: (&str, &str)) -> String {
    match found {
        Found::Pair {
            ours,
            theirs,
            checked,
        } => {
            let (Some(a), Some(b)) = (timelines.ours.get(*ours), timelines.theirs.get(*theirs))
            else {
                return crate::i18n::text("repo-core-0463").to_owned();
            };
            let mut text = crate::i18n::message(
                "repo-core-0464",
                &[
                    ("a", named(a).to_string().into()),
                    ("names", names.0.to_string().into()),
                    ("b", named(b).to_string().into()),
                    ("names2", names.1.to_string().into()),
                ],
            );
            text.push_str(if *checked {
                crate::i18n::text("repo-core-0465")
            } else {
                crate::i18n::text("repo-core-0466")
            });
            let cut =
                (*ours == 0 && timelines.truncated.0) || (*theirs == 0 && timelines.truncated.1);
            if cut {
                text.push_str(&crate::i18n::message(
                    "repo-core-0467",
                    &[(
                        "MAX-COMMITS",
                        format!("{}", super::super::git::MAX_COMMITS).into(),
                    )],
                ));
            }
            text
        }
        Found::OneSided => crate::i18n::text("repo-core-0468").to_owned(),
        Found::TipsMergeCleanly => crate::i18n::text("repo-core-0469").to_owned(),
    }
}

// A commit's change to the file, in place of the timelines.
