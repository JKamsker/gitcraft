//! Bounded commit-to-pull-request association and provider pagination.
use super::*;

pub(super) fn list(value: Value) -> Result<Vec<Value>, String> {
    value.as_array().cloned().ok_or_else(|| {
        crate::i18n::text("backend-forge-history-api-did-not-return-a-list-4383b5").into()
    })
}
pub(super) fn check_provider(forge: Forge) -> Result<(), String> {
    match forge {
        Forge::GitHub | Forge::GitLab | Forge::Gitea | Forge::Forgejo => Ok(()),
        Forge::Azure | Forge::Bitbucket => Err(crate::i18n::message(
            "backend-commit-to-pr-history-evidence-is-not-supported-for-open-its-pull-1f6d28",
            &[("name", (forge.name()).to_string().into())],
        )),
    }
}
pub(super) fn lookup_with(
    h: &Hosting,
    q: &Query,
    get: &mut dyn FnMut(&str) -> Result<Value, String>,
    prove: &mut dyn FnMut(&str, &str) -> bool,
) -> Result<Evidence, String> {
    check_provider(h.forge)?;
    let base = match h.forge {
        Forge::GitLab => format!("projects/{}", component(&h.path)),
        _ => format!("repos/{}", project(h)),
    };
    let endpoint = match h.forge {
        Forge::GitLab => format!(
            "{base}/repository/commits/{}/merge_requests?per_page=20",
            q.at.commit
        ),
        Forge::GitHub => format!("{base}/commits/{}/pulls?per_page=20", q.at.commit),
        _ => format!("{base}/commits/{}/pulls?limit=20", q.at.commit),
    };
    let associated = list(get(&endpoint)?)?;
    let mut result = Evidence {
        limited: associated.len() >= 4,
        ..Default::default()
    };
    for item in associated.into_iter().take(4) {
        let n = number(
            &item,
            if h.forge == Forge::GitLab {
                "/iid"
            } else {
                "/number"
            },
        );
        if n == 0 {
            result.notes.push(
                crate::i18n::text("backend-an-associated-pr-had-no-valid-number-113e31").into(),
            );
            continue;
        }
        let pr_path = if h.forge == Forge::GitLab {
            format!("{base}/merge_requests/{n}")
        } else {
            format!("{base}/pulls/{n}")
        };
        let details = match get(&pr_path) {
            Ok(v) => v,
            Err(e) => {
                result.notes.push(e);
                result.limited = true;
                continue;
            }
        };
        let merge = text(&details, "/merge_commit_sha");
        let squash = text(&details, "/squash_commit_sha");
        let mut pull = Pull {
            number: n,
            title: text(&details, "/title"),
            url: url(
                &details,
                if h.forge == Forge::GitLab {
                    "/web_url"
                } else {
                    "/html_url"
                },
            ),
            state: text(&details, "/state"),
            association: if merge == q.at.commit || squash == q.at.commit {
                crate::i18n::text("backend-exact-merge-squash-commit-e17e96").into()
            } else {
                crate::i18n::text(
                    "backend-forge-commit-association-multiple-prs-may-contain-this-version-dbc81f",
                )
                .into()
            },
            issues: issue_links(h, &text(&details, "/body")),
            comments: vec![],
            checked: 0,
            unmapped: 0,
            limited: false,
            notes: vec![],
        };
        if h.forge == Forge::GitLab {
            pull.issues = issue_links(h, &text(&details, "/description"));
        }
        if q.comments {
            let fetched = match h.forge {
                Forge::GitHub => {
                    // GitHub's LEFT coordinates belong to the three-dot merge
                    // base, which may precede the current base branch tip.
                    let base_sha = text(&details, "/base/sha");
                    let head_sha = text(&details, "/head/sha");
                    let left = if oid(&base_sha) && oid(&head_sha) {
                        get(&format!(
                            "{base}/compare/{base_sha}...{head_sha}?per_page=1"
                        ))
                        .ok()
                        .and_then(|v| github_merge_base(&v, &base_sha))
                    } else {
                        None
                    };
                    if left.is_none() {
                        pull.limited = true;
                        pull.notes.push(crate::i18n::text("backend-github-s-reviewed-merge-base-could-not-be-established-old-side-co-f06816").into());
                    }
                    pages(&format!("{pr_path}/comments?per_page=100"), get, &mut |v| {
                        github_comment(v, &details, left.as_deref(), q, prove, &mut pull)
                    })
                }
                Forge::GitLab => pages(
                    &format!("{pr_path}/discussions?per_page=100"),
                    get,
                    &mut |v| {
                        for note in v
                            .get("notes")
                            .and_then(Value::as_array)
                            .into_iter()
                            .flatten()
                            .take(100)
                        {
                            gitlab_comment(note, q, prove, &mut pull);
                        }
                    },
                ),
                Forge::Gitea | Forge::Forgejo => {
                    let reviews = get(&format!("{pr_path}/reviews?limit=20&page=1")).and_then(list);
                    match reviews {
                        Ok(reviews) => {
                            if reviews.len() >= 20 {
                                pull.limited = true;
                            }
                            let mut partial = false;
                            for review in reviews.into_iter().take(20) {
                                let id = number(&review, "/id");
                                if id == 0 {
                                    continue;
                                }
                                let revision = text(&review, "/commit_id");
                                match pages(
                                    &format!("{pr_path}/reviews/{id}/comments?limit=100"),
                                    get,
                                    &mut |v| gitea_comment(v, &revision, q, prove, &mut pull),
                                ) {
                                    Ok(limited) => partial |= limited,
                                    Err(error) => {
                                        pull.notes.push(error);
                                        partial = true;
                                        break;
                                    }
                                }
                            }
                            Ok(partial)
                        }
                        Err(error) => Err(error),
                    }
                }
                Forge::Azure | Forge::Bitbucket => {
                    return Err(crate::i18n::text(
                        "backend-this-forge-does-not-support-commit-to-pr-history-evidence-6f1ca1",
                    )
                    .into());
                }
            };
            match fetched {
                Ok(limited) => pull.limited |= limited,
                Err(error) => {
                    pull.notes.push(error);
                    pull.limited = true;
                }
            }
        }
        result.limited |= pull.limited;
        result.pulls.push(pull);
    }
    if result.limited {
        result.notes.push(
            crate::i18n::text(
                "backend-coverage-is-bounded-up-to4associated-prs-5comment-pages-per-endpo-9d92d5",
            )
            .into(),
        );
    }
    Ok(result)
}
