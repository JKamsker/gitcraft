//! Bounded, explicit commit→PR→line-comment evidence, shared by history cards.
//! No full PR/CI page is polled. Unmapped comments are counted, never relabelled as evidence.
use super::{Forge, Hosting};
use crate::git::investigations::{Reader, dossier::Seed, oid};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    path::Path,
    time::{Duration, Instant},
};
#[derive(Clone, Debug)]
pub(crate) struct Query {
    pub at: Seed,
    pub file_oid: String,
    pub comments: bool,
}
#[derive(Clone, Debug)]
pub(crate) struct Comment {
    pub author: String,
    pub body: String,
    pub url: String,
    pub proof: String,
}
#[derive(Clone, Debug)]
pub(crate) struct Pull {
    pub number: u64,
    pub title: String,
    pub url: String,
    pub state: String,
    pub association: String,
    pub issues: Vec<(String, String)>,
    pub comments: Vec<Comment>,
    pub checked: usize,
    pub unmapped: usize,
    pub limited: bool,
    pub notes: Vec<String>,
}
#[derive(Clone, Debug, Default)]
pub(crate) struct Evidence {
    pub pulls: Vec<Pull>,
    pub notes: Vec<String>,
    pub limited: bool,
}
fn text(v: &Value, key: &str) -> String {
    v.pointer(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .chars()
        .take(4096)
        .collect()
}
fn number(v: &Value, key: &str) -> u64 {
    v.pointer(key).and_then(Value::as_u64).unwrap_or(0)
}
fn component(value: &str) -> String {
    let mut s = String::new();
    for b in value.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            s.push(char::from(b));
        } else {
            s.push_str(&format!("%{b:02X}"));
        }
    }
    s
}
fn project(h: &Hosting) -> String {
    h.path
        .split('/')
        .map(component)
        .collect::<Vec<_>>()
        .join("/")
}
fn safe_url(url: &str) -> bool {
    let Some((scheme, rest)) = url.split_once("://") else {
        return false;
    };
    matches!(scheme, "https" | "http")
        && !rest.split('/').next().unwrap_or("").is_empty()
        && !rest.split('/').next().unwrap_or("").contains('@')
        && !url.chars().any(char::is_control)
}
fn url(v: &Value, key: &str) -> String {
    let value = text(v, key);
    if safe_url(&value) {
        value
    } else {
        String::new()
    }
}
fn issue_links(h: &Hosting, body: &str) -> Vec<(String, String)> {
    let words: Vec<_> = body.split_whitespace().take(2048).collect();
    let mut out = Vec::new();
    for pair in words.windows(2) {
        let verb = pair[0]
            .trim_matches(|c: char| !c.is_ascii_alphabetic())
            .to_ascii_lowercase();
        if !matches!(
            verb.as_str(),
            "fix"
                | "fixes"
                | "fixed"
                | "close"
                | "closes"
                | "closed"
                | "resolve"
                | "resolves"
                | "resolved"
        ) {
            continue;
        }
        let issue = pair[1].trim_matches(|c: char| matches!(c, ',' | '.' | ';' | ')' | ']'));
        let Some(n) = issue
            .strip_prefix('#')
            .and_then(|s| s.parse::<u64>().ok())
            .filter(|n| *n > 0)
        else {
            continue;
        };
        let segment = if h.forge == Forge::GitLab {
            "-/issues"
        } else {
            "issues"
        };
        let link = format!(
            "{}/{}/{segment}/{n}",
            h.web.trim_end_matches('/'),
            project(h)
        );
        if safe_url(&link) && !out.iter().any(|(id, _)| id == &format!("#{n}")) {
            out.push((format!("#{n}"), link));
        }
        if out.len() == 16 {
            break;
        }
    }
    out
}
struct Client<'a> {
    hosting: &'a Hosting,
    api: Option<super::http::Api>,
    began: Instant,
    requests: usize,
    active: &'a dyn Fn() -> bool,
}
impl Client<'_> {
    fn get(&mut self, path: &str) -> Result<Value, String> {
        if self.requests >= 48 || self.began.elapsed() > Duration::from_secs(90) || !(self.active)()
        {
            return Err("History evidence request budget reached or cancelled".into());
        }
        self.requests += 1;
        match &self.api {
            Some(api) => api.history_get(path, self.active),
            None => super::cli::history_api(self.hosting, path, self.active),
        }
    }
}
pub(crate) fn lookup(
    repo: &Path,
    hosting: &Hosting,
    query: &Query,
    active: &dyn Fn() -> bool,
) -> Result<Evidence, String> {
    check_provider(hosting.forge)?;
    if !oid(&query.at.commit)
        || !oid(&query.file_oid)
        || query.at.line == 0
        || query.at.line > 20000
        || query.at.path.len() > 4096
    {
        return Err(crate::i18n::text("backend-invalid-pinned-history-card-1b9013").into());
    }
    if !safe_url(&hosting.web)
        || hosting
            .path
            .split('/')
            .any(|s| s.is_empty() || matches!(s, "." | ".."))
    {
        return Err(crate::i18n::text("backend-invalid-captured-forge-repository-098e0a").into());
    }
    let authority = super::location(&format!("{}/_history_guard_", hosting.web))
        .map(|(host, _)| host)
        .ok_or(crate::i18n::text("backend-invalid-forge-authority-9d9b21"))?;
    if !authority.eq_ignore_ascii_case(&hosting.host) {
        return Err(
            crate::i18n::text("backend-captured-forge-account-and-api-host-differ-39ae5b").into(),
        );
    }
    let reader = Reader::new(repo, active);
    let config = reader.run(
        &["config", "--get-regexp", r"^remote\..*\.url$"],
        vec![],
        &[0, 1],
    )?;
    let config = std::str::from_utf8(&config)
        .map_err(|_| crate::i18n::text("backend-repository-remote-is-not-utf-8-db2d43"))?;
    let same = config
        .lines()
        .filter_map(|line| line.split_once(' '))
        .any(|(_, url)| {
            super::location(url).is_some_and(|(host, path)| {
                host.eq_ignore_ascii_case(&hosting.host) && path == hosting.path
            })
        });
    if !same {
        return Err(crate::i18n::text(
            "backend-repository-forge-remote-changed-refresh-before-requesting-evidenc-cd608a",
        )
        .into());
    }
    let api = matches!(hosting.forge, Forge::Gitea | Forge::Forgejo)
        .then(|| super::gitea::history_api(hosting));
    let mut client = Client {
        hosting,
        api,
        began: Instant::now(),
        requests: 0,
        active,
    };
    let mut proved = BTreeMap::<(String, String), bool>::new();
    let mut proof_limited = false;
    let mut prove = |revision: &str, path: &str| {
        if path != query.at.path || !oid(revision) {
            return false;
        }
        if revision == query.at.commit {
            return true;
        }
        let key = (revision.to_owned(), path.to_owned());
        if let Some(result) = proved.get(&key) {
            return *result;
        }
        if proved.len() >= 64 {
            proof_limited = true;
            return false;
        }
        let proof = (|| {
            let out = reader
                .run(
                    &[
                        "--literal-pathspecs",
                        "ls-tree",
                        "-z",
                        "--full-tree",
                        revision,
                        "--",
                        path,
                    ],
                    vec![],
                    &[0],
                )
                .ok()?;
            let records: Vec<_> = out.split(|b| *b == 0).filter(|s| !s.is_empty()).collect();
            let [record] = records.as_slice() else {
                return None;
            };
            let split = record.iter().position(|b| *b == b'\t')?;
            if &record[split + 1..] != path.as_bytes() {
                return None;
            }
            let header = std::str::from_utf8(&record[..split]).ok()?;
            let fields: Vec<_> = header.split_whitespace().collect();
            let [mode, kind, id] = fields.as_slice() else {
                return None;
            };
            Some(matches!(*mode, "100644" | "100755") && *kind == "blob" && *id == query.file_oid)
        })()
        .unwrap_or(false);
        proved.insert(key, proof);
        proof
    };
    let mut result = lookup_with(hosting, query, &mut |path| client.get(path), &mut prove)?;
    if proof_limited {
        result.limited = true;
        result.notes.push(
            crate::i18n::text(
                "backend-64-reviewed-revision-proofs-reached-remaining-comments-were-not-l-e73501",
            )
            .into(),
        );
    }
    Ok(result)
}
fn list(value: Value) -> Result<Vec<Value>, String> {
    value.as_array().cloned().ok_or_else(|| {
        crate::i18n::text("backend-forge-history-api-did-not-return-a-list-4383b5").into()
    })
}
fn check_provider(forge: Forge) -> Result<(), String> {
    match forge {
        Forge::GitHub | Forge::GitLab | Forge::Gitea | Forge::Forgejo => Ok(()),
        Forge::Azure | Forge::Bitbucket => Err(crate::i18n::message(
            "backend-commit-to-pr-history-evidence-is-not-supported-for-open-its-pull-1f6d28",
            &[("name", (forge.name()).to_string().into())],
        )),
    }
}
fn lookup_with(
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
fn pages(
    endpoint: &str,
    get: &mut dyn FnMut(&str) -> Result<Value, String>,
    visit: &mut dyn FnMut(&Value),
) -> Result<bool, String> {
    let mut previous = String::new();
    for page in 1..=5 {
        let rows = list(get(&format!("{endpoint}&page={page}"))?)?;
        if rows.is_empty() {
            return Ok(false);
        }
        let marker = rows.first().map(Value::to_string).unwrap_or_default();
        if page > 1 && marker == previous {
            return Ok(true);
        }
        previous = marker;
        for row in rows.iter().take(100) {
            visit(row);
        }
        if rows.len() > 100 {
            return Ok(true);
        }
    }
    Ok(true)
}
fn range(v: &Value, end: &str, start: &str) -> Option<(usize, usize)> {
    let end = usize::try_from(number(v, end)).ok()?;
    let begin = usize::try_from(number(v, start))
        .ok()
        .filter(|n| *n > 0)
        .unwrap_or(end);
    (begin > 0 && begin <= end && end <= 20000).then_some((begin, end))
}
// Each evidence record carries independent author/body/link/provenance fields.
#[allow(clippy::too_many_arguments)]
fn add(
    v: &Value,
    anchors: Vec<(String, String, usize, usize)>,
    author: &str,
    body: &str,
    link: &str,
    q: &Query,
    prove: &mut dyn FnMut(&str, &str) -> bool,
    pull: &mut Pull,
) {
    pull.checked += 1;
    let proof = anchors
        .into_iter()
        .find_map(|(revision, path, start, end)| {
            (path == q.at.path
                && start <= q.at.line
                && q.at.line <= end
                && oid(&revision)
                && prove(&revision, &path))
            .then(|| {
                if revision == q.at.commit {
                    crate::i18n::text("backend-exact-reviewed-revision-and-line-range-522601").into()
                } else {
                    crate::i18n::text("backend-identical-pinned-file-blob-at-the-reviewed-revision-line-numbers-fd3258")
                        .into()
                }
            })
        });
    let Some(proof) = proof else {
        pull.unmapped += 1;
        return;
    };
    if pull.comments.len() >= 32
        || pull.comments.iter().map(|c| c.body.len()).sum::<usize>() > 128 * 1024
    {
        pull.limited = true;
        return;
    }
    let body = text(v, body);
    let link = url(v, link);
    if !link.is_empty() && pull.comments.iter().any(|c| c.url == link) {
        return;
    }
    pull.comments.push(Comment {
        author: text(v, author),
        body,
        url: link,
        proof,
    });
}
fn github_merge_base(comparison: &Value, base: &str) -> Option<String> {
    let merge_base = text(comparison, "/merge_base_commit/sha");
    (text(comparison, "/base_commit/sha") == base
        && matches!(text(comparison, "/status").as_str(), "ahead" | "diverged")
        && number(comparison, "/ahead_by") > 0
        && oid(&merge_base))
    .then_some(merge_base)
}
fn github_comment(
    v: &Value,
    pr: &Value,
    merge_base: Option<&str>,
    q: &Query,
    prove: &mut dyn FnMut(&str, &str) -> bool,
    pull: &mut Pull,
) {
    let mut anchors = Vec::new();
    let path = text(v, "/path");
    let side = text(v, "/side");
    if side == "RIGHT" {
        for (rev, line, start) in [
            ("/commit_id", "/line", "/start_line"),
            (
                "/original_commit_id",
                "/original_line",
                "/original_start_line",
            ),
        ] {
            if let Some((start, end)) = range(v, line, start) {
                let start = if text(v, "/start_side").is_empty() || text(v, "/start_side") == side {
                    start
                } else {
                    end
                };
                anchors.push((text(v, rev), path.clone(), start, end));
            }
        }
    } else if side == "LEFT"
        && text(v, "/commit_id") == text(pr, "/head/sha")
        && let Some(base) = merge_base.filter(|base| oid(base))
        && let Some((start, end)) = range(v, "/line", "/start_line")
    {
        let start = if text(v, "/start_side").is_empty() || text(v, "/start_side") == side {
            start
        } else {
            end
        };
        anchors.push((base.to_owned(), path, start, end));
    }
    add(
        v,
        anchors,
        "/user/login",
        "/body",
        "/html_url",
        q,
        prove,
        pull,
    );
}
fn gitlab_comment(
    v: &Value,
    q: &Query,
    prove: &mut dyn FnMut(&str, &str) -> bool,
    pull: &mut Pull,
) {
    if v.get("system").and_then(Value::as_bool) == Some(true) {
        return;
    }
    let mut anchors = Vec::new();
    for (revision, path, line, start) in [
        (
            "/position/head_sha",
            "/position/new_path",
            "/position/new_line",
            "/position/line_range/start/new_line",
        ),
        (
            "/position/base_sha",
            "/position/old_path",
            "/position/old_line",
            "/position/line_range/start/old_line",
        ),
    ] {
        if let Some((start, end)) = range(v, line, start) {
            anchors.push((text(v, revision), text(v, path), start, end));
        }
    }
    add(
        v,
        anchors,
        "/author/username",
        "/body",
        "/url",
        q,
        prove,
        pull,
    );
}
fn gitea_comment(
    v: &Value,
    review: &str,
    q: &Query,
    prove: &mut dyn FnMut(&str, &str) -> bool,
    pull: &mut Pull,
) {
    let path = text(v, "/path");
    let new = number(v, "/position");
    let old = number(v, "/original_position");
    let mut anchors = Vec::new();
    if new > 0 && new <= 20000 {
        let revision = if oid(review) {
            review.to_owned()
        } else {
            text(v, "/commit_id")
        };
        anchors.push((revision, path.clone(), new as usize, new as usize));
    } else if old > 0 && old <= 20000 {
        // original_position is an old-side source line, not a line at the
        // review head. Gitea/Forgejo serialize its OldRef separately.
        anchors.push((
            text(v, "/original_commit_id"),
            path,
            old as usize,
            old as usize,
        ));
    }
    add(
        v,
        anchors,
        "/user/login",
        "/body",
        "/html_url",
        q,
        prove,
        pull,
    );
}
#[cfg(test)]
mod tests;
