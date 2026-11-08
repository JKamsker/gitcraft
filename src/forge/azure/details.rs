use super::*;
use crate::{
    pr_threads::{PrExtras, ReviewFeatures, Thread, ThreadComment},
    pull_request::{
        Check, CheckState, DiffSide, Entry, EntryKind, PrCommit, PrDetails, items, parse_time,
    },
};
pub(super) struct Snapshot {
    pub repo: Repository,
    pub pr: Value,
    pub iteration: Value,
}
impl Snapshot {
    pub fn endpoint(&self, n: u64) -> String {
        format!("{}/pullrequests/{n}", self.repo.endpoint)
    }
    pub fn iteration_id(&self) -> u64 {
        self.iteration
            .get("id")
            .and_then(Value::as_u64)
            .unwrap_or(0)
    }
    pub fn revision(&self) -> String {
        json!({"repository":self.repo.id,"project":self.repo.project,"number":self.pr["pullRequestId"],"sourceRepository":self.pr.pointer("/forkSource/repository/id"),"sourceBranch":self.pr["sourceRefName"],"targetBranch":self.pr["targetRefName"],"source":self.pr["lastMergeSourceCommit"],"target":self.pr["lastMergeTargetCommit"],"iteration":self.iteration_id(),"iterationTarget":self.iteration["targetRefCommit"],"common":self.iteration["commonRefCommit"]}).to_string()
    }
}
pub(super) fn snapshot(api: &Api, n: u64) -> Result<Snapshot, String> {
    let repo = repository(api)?;
    let endpoint = format!("{}/pullrequests/{n}", repo.endpoint);
    let pr = api.get(&endpoint)?;
    if pr.get("pullRequestId").and_then(Value::as_u64) != Some(n)
        || required(&pr, "/repository/id")? != repo.id
        || required(&pr, "/repository/project/id")? != repo.project
    {
        return Err("Azure returned a different pull request/repository/project".into());
    }
    let iteration = api
        .array(&format!("{endpoint}/iterations"))?
        .into_iter()
        .max_by_key(|v| v.get("id").and_then(Value::as_u64))
        .ok_or("Azure has not returned a reviewable iteration")?;
    let s = Snapshot {
        repo,
        pr,
        iteration,
    };
    for p in [
        "/sourceRefCommit/commitId",
        "/targetRefCommit/commitId",
        "/commonRefCommit/commitId",
    ] {
        if !crate::pull_request::is_oid(required(&s.iteration, p)?) {
            return Err("Azure iteration has an invalid commit".into());
        }
    }
    if s.iteration_id() == 0
        || text(&s.iteration, "/sourceRefCommit/commitId")
            != text(&s.pr, "/lastMergeSourceCommit/commitId")
        || !crate::pull_request::is_oid(required(&s.pr, "/lastMergeTargetCommit/commitId")?)
    {
        return Err("Azure is still updating the PR iteration; reload before reviewing".into());
    }
    Ok(s)
}
pub(super) fn read(api: &Api, n: u64) -> Result<PrDetails, String> {
    for _ in 0..2 {
        let s = snapshot(api, n)?;
        let end = s.endpoint(n);
        let iteration = s.iteration_id();
        let threads = api.array(&format!(
            "{end}/threads?$iteration={iteration}&$baseIteration={iteration}"
        ));
        let commits = api.array(&format!("{end}/commits?$top=1000"));
        let statuses = api.array(&format!("{end}/statuses"));
        let policies = api.array(&format!(
            "{}/_apis/policy/evaluations?artifactId={}",
            segment(&s.repo.project),
            segment(&format!(
                "vstfs:///CodeReview/CodeReviewId/{}/{n}",
                s.repo.project
            ))
        ));
        let after = snapshot(api, n)?;
        api.verify_viewer()?;
        if s.revision() != after.revision() || s.pr["status"] != after.pr["status"] {
            continue;
        }
        let parsed = parse(&s.pr, &api.hosting)?;
        let open = text(&s.pr, "/status") == "active";
        let own = text(&s.pr, "/createdBy/id").eq_ignore_ascii_case(&api.viewer);
        let mut extras = PrExtras {
            pr_id: n.to_string(),
            native_policy: true,
            native_account: Some(api.viewer.clone()),
            viewer: Some(if own {
                parsed.author.clone()
            } else {
                api.viewer.clone()
            }),
            head_oid: parsed.sha.clone(),
            review_features: ReviewFeatures {
                pending: false,
                request_changes: open && !own,
                approve: open && !own,
                apply_suggestions: false,
            },
            connection_binding: Some(crate::pr_threads::connection_binding()),
            review_revision: Some(s.revision()),
            merge_blocker: Some(
                "Complete this pull request on Azure DevOps; in-app completion is unavailable."
                    .into(),
            ),
            merge_handoff: true,
            ..Default::default()
        };
        let mut timeline = vec![];
        for reviewer in items(&s.pr, "/reviewers") {
            let vote = reviewer.get("vote").and_then(Value::as_i64).unwrap_or(0);
            if vote != 0 {
                timeline.push(Entry {
                    kind: EntryKind::Review(if vote > 0 {
                        "approved"
                    } else {
                        "requested changes"
                    }),
                    author: text(reviewer, "/displayName"),
                    at: 0,
                    body: format!("Azure review vote: {vote}"),
                    url: parsed.url.clone(),
                    review_id: None,
                });
            }
        }
        if let Ok(rows) = &threads {
            for row in rows {
                if row.get("isDeleted").and_then(Value::as_bool) == Some(true) {
                    continue;
                }
                let thread = thread(row, iteration, open);
                if !thread.path.is_empty() {
                    extras.threads.push(thread);
                } else {
                    for c in &thread.comments {
                        timeline.push(Entry {
                            kind: EntryKind::Comment,
                            author: c.author.clone(),
                            at: c.at,
                            body: c.body.clone(),
                            url: c.url.clone(),
                            review_id: None,
                        });
                    }
                    extras.threads.push(thread);
                }
            }
        }
        timeline.extend(
            crate::pull_request::line_entries(&extras)
                .into_iter()
                .filter(|e| matches!(&e.kind,EntryKind::Inline{path,..} if !path.is_empty())),
        );
        timeline.sort_by_key(|e| e.at);
        let mut checks = vec![];
        if let Ok(rows) = &statuses {
            for v in rows {
                checks.push(Check {
                    name: text(v, "/context/name"),
                    state: state(&text(v, "/state")),
                    detail: text(v, "/description"),
                    url: text(v, "/targetUrl"),
                });
            }
        }
        if let Ok(rows) = &policies {
            for v in rows {
                checks.push(Check {
                    name: text(v, "/configuration/type/displayName"),
                    state: state(&text(v, "/status")),
                    detail: if v
                        .pointer("/configuration/isBlocking")
                        .and_then(Value::as_bool)
                        == Some(true)
                    {
                        "Required Azure policy".into()
                    } else {
                        "Azure policy".into()
                    },
                    url: parsed.url.clone(),
                });
            }
        }
        let errors = [
            statuses.as_ref().err(),
            policies.as_ref().err(),
            commits.as_ref().err(),
        ]
        .into_iter()
        .flatten()
        .cloned()
        .collect::<Vec<_>>();
        return Ok(PrDetails {
            forge: Some(Forge::Azure),
            number: n,
            title: parsed.title,
            body: text(&s.pr, "/description"),
            state: if open {
                "OPEN"
            } else if text(&s.pr, "/status") == "completed" {
                "MERGED"
            } else {
                "CLOSED"
            }
            .into(),
            draft: parsed.draft,
            author: parsed.author,
            url: parsed.url,
            base_branch: parsed.base,
            head_branch: parsed.branch,
            base_oid: text(&s.iteration, "/commonRefCommit/commitId"),
            head_oid: parsed.sha,
            head_ref: format!("refs/pull/{n}/merge"),
            created_at: parse_time(&text(&s.pr, "/creationDate")).unwrap_or(0),
            checks,
            checks_error: (!errors.is_empty()).then(|| errors.join("; ")),
            commits: commits
                .unwrap_or_default()
                .iter()
                .map(|v| PrCommit {
                    oid: text(v, "/commitId"),
                    headline: text(v, "/comment")
                        .lines()
                        .next()
                        .unwrap_or_default()
                        .into(),
                    author: text(v, "/author/name"),
                })
                .collect(),
            timeline,
            inline_error: threads.err(),
            merge_state: text(&s.pr, "/mergeStatus"),
            extras,
            ..Default::default()
        });
    }
    Err("Azure pull request changed while loading; reload it".into())
}
fn state(s: &str) -> CheckState {
    match s.to_ascii_lowercase().as_str() {
        "approved" | "succeeded" => CheckState::Passed,
        "rejected" | "failed" | "error" | "broken" => CheckState::Failed,
        "queued" | "running" | "pending" => CheckState::Running,
        _ => CheckState::Skipped,
    }
}
fn thread(v: &Value, iteration: u64, open: bool) -> Thread {
    let status = text(v, "/status");
    let resolved = matches!(
        status.as_str(),
        "fixed" | "wontFix" | "closed" | "byDesign" | "2" | "3" | "4" | "5"
    );
    let right = v
        .pointer("/threadContext/rightFileStart/line")
        .and_then(Value::as_u64);
    let line = right.or_else(|| {
        v.pointer("/threadContext/leftFileStart/line")
            .and_then(Value::as_u64)
    });
    let original = v
        .pointer("/pullRequestThreadContext/iterationContext/secondComparingIteration")
        .and_then(Value::as_u64);
    Thread {
        id: v.get("id").map(|v| v.to_string()).unwrap_or_default(),
        path: text(v, "/threadContext/filePath")
            .trim_start_matches('/')
            .into(),
        line,
        start_line: None,
        side: if right.is_some() {
            DiffSide::Right
        } else {
            DiffSide::Left
        },
        outdated: original.is_some_and(|i| i != iteration),
        resolved,
        can_resolve: open && !resolved,
        can_unresolve: open && resolved,
        can_reply: open,
        comments: items(v, "/comments")
            .iter()
            .filter(|c| c.get("isDeleted").and_then(Value::as_bool) != Some(true))
            .map(|c| ThreadComment {
                id: c.get("id").map(|v| v.to_string()).unwrap_or_default(),
                author: text(c, "/author/displayName"),
                body: text(c, "/content"),
                at: parse_time(&text(c, "/publishedDate")).unwrap_or(0),
                url: String::new(),
                pending: false,
            })
            .collect(),
        starts_left: false,
        more_comments: false,
    }
}
