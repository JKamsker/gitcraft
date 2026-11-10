//! Localized rendering of retained approval facts.

use super::*;

pub(super) fn display(kind: &AskPresentation) -> Ask {
    match kind.clone() {
        AskPresentation::Stage { conflicted } => Ask {
            title: crate::i18n::text("repo-workflows-agent-stage-17e7b517").to_owned(),
            undo_note: crate::i18n::text(NO_UNDO).to_owned(),
            body: crate::i18n::message(
                "repo-workflows-agent-stage-conflicts",
                &[
                    ("count", (conflicted.len()).into()),
                    ("paths", (conflicted.join(", ")).into()),
                ],
            ),
            presentation: None,
        },
        AskPresentation::HardReset {
            target,
            from,
            to,
            subject,
            lost,
            left,
        } => Ask {
            title: crate::i18n::message(
                "repo-workflows-agent-hard-ask-e720ce31",
                &[
                    ("target", (target).clone().into()),
                    ("short-hash-1", (to.clone()).clone().into()),
                ],
            ),
            body: crate::i18n::message(
                "repo-workflows-agent-hard-reset-body",
                &[
                    ("target", (target.clone()).into()),
                    ("from", (from.clone()).into()),
                    ("to", (to.clone()).into()),
                    ("subject", (subject.clone()).into()),
                    ("lost", (lost).into()),
                    ("left", (left).into()),
                ],
            ),
            undo_note: crate::i18n::message(
                "repo-workflows-agent-hard-ask-8de2bf44",
                &[("target", (target).clone().into())],
            ),
            presentation: None,
        },
        AskPresentation::MixedReset {
            target,
            from,
            to,
            subject,
            files,
        } => Ask {
            title: crate::i18n::message(
                "repo-workflows-agent-mixed-ask-2c1fd48b",
                &[
                    ("target", (target).clone().into()),
                    ("short-hash-1", (to.clone()).clone().into()),
                ],
            ),
            body: crate::i18n::message(
                "repo-workflows-agent-mixed-reset-body",
                &[
                    ("target", (target.clone()).into()),
                    ("from", (from.clone()).into()),
                    ("to", (to.clone()).into()),
                    ("subject", (subject.clone()).into()),
                    ("files", (files).into()),
                ],
            ),
            undo_note: crate::i18n::message(
                "repo-workflows-agent-mixed-ask-2ddd09f5",
                &[("target", (target).clone().into())],
            ),
            presentation: None,
        },
        AskPresentation::Drop {
            commit,
            subject,
            branch,
            after,
        } => Ask {
            title: crate::i18n::message(
                "repo-workflows-agent-rewrite-fc4198eb",
                &[("short-hash-1", (commit.clone()).clone().into())],
            ),
            body: crate::i18n::message(
                "repo-workflows-agent-drop-body",
                &[
                    ("commit", (commit.clone()).into()),
                    ("subject", (subject.clone()).into()),
                    ("branch", (branch.as_str()).into()),
                    ("after", (after).into()),
                ],
            ),
            undo_note: crate::i18n::message(
                "repo-workflows-agent-rewrite-177f568d",
                &[("branch", (branch).clone().into())],
            ),
            presentation: None,
        },
        AskPresentation::DeleteBranch {
            name,
            commit,
            subject,
            force,
        } => Ask {
            title: crate::i18n::message(
                "repo-workflows-agent-delete-branch-d32d800a",
                &[("name", (name).clone().into())],
            ),
            body: crate::i18n::message(
                "repo-workflows-agent-delete-body",
                &[
                    ("name", (name.clone()).into()),
                    ("commit", (commit.clone()).into()),
                    ("subject", (subject.clone()).into()),
                    ("force", (if force { "yes" } else { "no" }).into()),
                ],
            ),
            undo_note: crate::i18n::text("repo-workflows-agent-delete-branch-7ef8245e").to_owned(),
            presentation: None,
        },
        AskPresentation::Push {
            branch,
            remote,
            there,
            publish,
            force,
            ahead,
            behind,
        } => {
            let to = format!("{remote}/{there}");
            let (title, body) = if publish {
                (
                    crate::i18n::message(
                        "repo-workflows-agent-push-7d6400fd",
                        &[
                            ("branch", (branch).clone().into()),
                            ("remote", (remote).clone().into()),
                        ],
                    ),
                    crate::i18n::message(
                        "repo-workflows-agent-push-747bd6a7",
                        &[
                            ("branch", (branch).clone().into()),
                            ("remote", (remote).clone().into()),
                            ("there", (there).clone().into()),
                        ],
                    ),
                )
            } else if force {
                let gone = match behind {
                    0 => crate::i18n::message(
                        "repo-workflows-agent-push-a74bf451",
                        &[
                            ("to", (to).clone().into()),
                            ("branch", (branch).clone().into()),
                        ],
                    ),
                    n => crate::i18n::message(
                        "repo-workflows-agent-force-lost",
                        &[
                            ("count", (n).into()),
                            ("to", (to.as_str()).into()),
                            ("branch", (branch.as_str()).into()),
                        ],
                    ),
                };
                (
                    crate::i18n::message(
                        "repo-workflows-agent-push-ccafdcaa",
                        &[
                            ("branch", (branch).clone().into()),
                            ("to", (to).clone().into()),
                        ],
                    ),
                    crate::i18n::message(
                        "repo-workflows-agent-push-56d04849",
                        &[
                            ("to", (to).clone().into()),
                            ("branch", (branch).clone().into()),
                            ("gone", (gone).clone().into()),
                        ],
                    ),
                )
            } else {
                (
                    crate::i18n::message(
                        "repo-workflows-agent-push-cb055d06",
                        &[
                            ("branch", (branch).clone().into()),
                            ("to", (to).clone().into()),
                        ],
                    ),
                    crate::i18n::message(
                        "repo-workflows-agent-push-body",
                        &[
                            ("branch", (branch.as_str()).into()),
                            ("count", (ahead).into()),
                            ("to", (to.as_str()).into()),
                        ],
                    ),
                )
            };
            Ask {
                title,
                body,
                undo_note: crate::i18n::text(NO_UNDO).to_owned(),
                presentation: None,
            }
        }
        AskPresentation::Fallback { label } => Ask {
            title: label.clone(),
            body: label,
            undo_note: crate::i18n::text(NO_UNDO).to_owned(),
            presentation: None,
        },
    }
}
