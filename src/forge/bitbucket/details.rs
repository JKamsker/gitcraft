use super::*;
use crate::{
    pr_threads::{PrExtras, ReviewFeatures, Thread, ThreadComment},
    pull_request::{Check, CheckState, DiffSide, Entry, EntryKind, PrCommit, items, parse_time},
};
pub(super) struct Snapshot {
    pub pr: Value,
    pub repo: Value,
}
impl Snapshot {
    pub fn revision(&self) -> String {
        json!({"repo":self.repo["uuid"],"number":self.pr["id"],"source":self.pr["source"],"destination":self.pr["destination"]}).to_string()
    }
    pub fn end(&self, api: &Api) -> Result<String, String> {
        Ok(format!(
            "{}/pullrequests/{}",
            endpoint(&api.hosting)?,
            self.pr["id"]
        ))
    }
}
pub(super) fn snapshot(api: &Api, n: u64) -> Result<Snapshot, String> {
    let repo = repository(api)?;
    let pr = api.get(&format!("{}/pullrequests/{n}", endpoint(&api.hosting)?))?;
    if pr.get("id").and_then(Value::as_u64) != Some(n)
        || pr.pointer("/destination/repository/uuid") != repo.get("uuid")
    {
        return Err(crate::i18n::text(
            "backend-bitbucket-returned-a-different-pr-repository-e2606a",
        )
        .into());
    }
    for p in ["/source/commit/hash", "/destination/commit/hash"] {
        if !crate::pull_request::is_oid(required(&pr, p)?) {
            return Err(crate::i18n::text(
                "backend-bitbucket-returned-an-invalid-commit-id-7a2d6b",
            )
            .into());
        }
    }
    required(&pr, "/source/repository/uuid")?;
    Ok(Snapshot { pr, repo })
}
pub(super) fn read(api: &Api, n: u64) -> Result<PrDetails, String> {
    for _ in 0..2 {
        let s = snapshot(api, n)?;
        let p = parse(&s.pr, &api.hosting)?;
        let end = s.end(api)?;
        let common = api.get(&format!(
            "{}/merge-base/{}..{}",
            endpoint(&api.hosting)?,
            text(&s.pr, "/destination/commit/hash"),
            p.sha
        ))?;
        let base = required(&common, "/hash")?.to_owned();
        if !crate::pull_request::is_oid(&base) {
            return Err(crate::i18n::text(
                "backend-bitbucket-returned-an-invalid-common-ancestor-4d3940",
            )
            .into());
        }
        let comments = pages(api, &format!("{end}/comments?pagelen=100"));
        let commits = pages(api, &format!("{end}/commits?pagelen=100"));
        let statuses = pages(api, &format!("{end}/statuses?pagelen=100"));
        let diffstat = diffstat(api, &base, &p.sha);
        let after = snapshot(api, n)?;
        api.verify_viewer()?;
        if s.revision() != after.revision() || s.pr["state"] != after.pr["state"] {
            continue;
        }
        let open = text(&s.pr, "/state") == "OPEN";
        let own = text(&s.pr, "/author/uuid").eq_ignore_ascii_case(&api.viewer);
        let mut extras=PrExtras{pr_id:n.to_string(),native_policy:true,native_account:Some(api.viewer.clone()),viewer:Some(if own{p.author.clone()}else{api.viewer.clone()}),head_oid:p.sha.clone(),head_repo:Some(text(&s.pr,"/source/repository/full_name")),connection_binding:Some(crate::pr_threads::connection_binding()),review_revision:Some(s.revision()),review_features:ReviewFeatures{pending:false,request_changes:open&&!own,approve:open&&!own,apply_suggestions:false},merge_handoff:true,merge_blocker:Some(crate::i18n::text("backend-bitbucket-cloud-s-documented-merge-api-has-no-expected-head-condi-3cd48b").into()),..Default::default()};
        let mut timeline = vec![];
        for v in items(&s.pr, "/participants") {
            let status = text(v, "/state");
            if v.get("approved").and_then(Value::as_bool) == Some(true)
                || status == "changes_requested"
            {
                timeline.push(Entry {
                    kind: EntryKind::Review(if status == "changes_requested" {
                        crate::i18n::text("backend-requested-changes-7adfe7")
                    } else {
                        "approved"
                    }),
                    author: text(v, "/user/display_name"),
                    at: parse_time(&text(v, "/participated_on")).unwrap_or(0),
                    body: String::new(),
                    url: p.url.clone(),
                    review_id: None,
                });
            }
        }
        if let Ok(rows) = &comments {
            extras.threads = threads(rows, open);
            for t in &extras.threads {
                if t.path.is_empty() {
                    for c in &t.comments {
                        timeline.push(Entry {
                            kind: EntryKind::Comment,
                            author: c.author.clone(),
                            body: c.body.clone(),
                            at: c.at,
                            url: c.url.clone(),
                            review_id: None,
                        });
                    }
                }
            }
        }
        timeline.extend(
            crate::pull_request::line_entries(&extras)
                .into_iter()
                .filter(|e| matches!(&e.kind,EntryKind::Inline{path,..} if !path.is_empty())),
        );
        timeline.sort_by_key(|e| e.at);
        let checks = statuses
            .as_ref()
            .map(|rows| {
                rows.iter()
                    .map(|v| Check {
                        name: text(v, "/name"),
                        detail: text(v, "/description"),
                        url: text(v, "/url"),
                        state: match text(v, "/state").as_str() {
                            "SUCCESSFUL" => CheckState::Passed,
                            "FAILED" | "ERROR" => CheckState::Failed,
                            "INPROGRESS" => CheckState::Running,
                            _ => CheckState::Skipped,
                        },
                    })
                    .collect()
            })
            .unwrap_or_default();
        let errors = [
            statuses.as_ref().err(),
            commits.as_ref().err(),
            diffstat.as_ref().err(),
        ]
        .into_iter()
        .flatten()
        .cloned()
        .collect::<Vec<_>>();
        let counts = diffstat
            .as_ref()
            .map(|rows| {
                (
                    rows.len() as u64,
                    rows.iter()
                        .filter_map(|v| v.get("lines_added").and_then(Value::as_u64))
                        .sum(),
                    rows.iter()
                        .filter_map(|v| v.get("lines_removed").and_then(Value::as_u64))
                        .sum(),
                )
            })
            .unwrap_or_default();
        return Ok(PrDetails {
            forge: Some(Forge::Bitbucket),
            number: n,
            title: p.title,
            body: text(&s.pr, "/description"),
            state: text(&s.pr, "/state"),
            draft: p.draft,
            author: p.author,
            url: p.url,
            head_branch: p.branch,
            base_branch: p.base,
            head_oid: p.sha,
            base_oid: base,
            head_ref: format!("refs/heads/{}", text(&s.pr, "/source/branch/name")),
            created_at: parse_time(&text(&s.pr, "/created_on")).unwrap_or(0),
            updated_at: parse_time(&text(&s.pr, "/updated_on")).unwrap_or(0),
            changed_files: counts.0,
            additions: counts.1,
            deletions: counts.2,
            checks,
            checks_error: (!errors.is_empty()).then(|| errors.join("; ")),
            commits: commits
                .unwrap_or_default()
                .iter()
                .map(|v| PrCommit {
                    oid: text(v, "/hash"),
                    headline: text(v, "/message")
                        .lines()
                        .next()
                        .unwrap_or_default()
                        .into(),
                    author: text(v, "/author/user/display_name"),
                })
                .collect(),
            inline_error: comments.err(),
            timeline,
            extras,
            ..Default::default()
        });
    }
    Err(
        crate::i18n::text("backend-bitbucket-source-or-target-changed-while-reading-reload-d92e43")
            .into(),
    )
}
fn threads(rows: &[Value], open: bool) -> Vec<Thread> {
    let mut roots = std::collections::BTreeMap::<u64, Vec<&Value>>::new();
    let ids = rows
        .iter()
        .filter_map(|v| v.get("id").and_then(Value::as_u64))
        .collect::<HashSet<_>>();
    for v in rows {
        if v.get("deleted").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        let Some(id) = v.get("id").and_then(Value::as_u64) else {
            continue;
        };
        let mut root = id;
        let mut cursor = v;
        let mut seen = HashSet::new();
        while let Some(parent) = cursor
            .pointer("/parent/id")
            .and_then(Value::as_u64)
            .filter(|p| ids.contains(p))
        {
            if !seen.insert(parent) {
                break;
            }
            root = parent;
            let Some(next) = rows
                .iter()
                .find(|v| v.get("id").and_then(Value::as_u64) == Some(parent))
            else {
                break;
            };
            cursor = next;
        }
        roots.entry(root).or_default().push(v);
    }
    roots
        .into_iter()
        .map(|(id, mut comments)| {
            comments.sort_by_key(|v| v.get("id").and_then(Value::as_u64));
            let root = rows
                .iter()
                .find(|v| v.get("id").and_then(Value::as_u64) == Some(id))
                .unwrap_or(&Value::Null);
            let resolved = root.get("resolution").is_some_and(|v| !v.is_null());
            let right = root.pointer("/inline/to").and_then(Value::as_u64);
            Thread {
                id: id.to_string(),
                path: text(root, "/inline/path"),
                line: right.or_else(|| root.pointer("/inline/from").and_then(Value::as_u64)),
                start_line: None,
                side: if right.is_some() {
                    DiffSide::Right
                } else {
                    DiffSide::Left
                },
                outdated: root.pointer("/inline/outdated").and_then(Value::as_bool) == Some(true),
                resolved,
                can_resolve: open && !resolved,
                can_unresolve: open && resolved,
                can_reply: open,
                comments: comments
                    .into_iter()
                    .map(|c| ThreadComment {
                        id: c["id"].to_string(),
                        author: text(c, "/user/display_name"),
                        body: text(c, "/content/raw"),
                        at: parse_time(&text(c, "/created_on")).unwrap_or(0),
                        url: text(c, "/links/html/href"),
                        pending: false,
                    })
                    .collect(),
                starts_left: false,
                more_comments: false,
            }
        })
        .collect()
}
