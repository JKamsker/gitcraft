//! Provider comment coordinates and pinned-revision evidence mapping.
use super::*;

pub(super) fn range(v: &Value, end: &str, start: &str) -> Option<(usize, usize)> {
    let end = usize::try_from(number(v, end)).ok()?;
    let begin = usize::try_from(number(v, start))
        .ok()
        .filter(|n| *n > 0)
        .unwrap_or(end);
    (begin > 0 && begin <= end && end <= 20000).then_some((begin, end))
}
// Each evidence record carries independent author/body/link/provenance fields.
#[allow(clippy::too_many_arguments)]
pub(super) fn add(
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
pub(super) fn github_merge_base(comparison: &Value, base: &str) -> Option<String> {
    let merge_base = text(comparison, "/merge_base_commit/sha");
    (text(comparison, "/base_commit/sha") == base
        && matches!(text(comparison, "/status").as_str(), "ahead" | "diverged")
        && number(comparison, "/ahead_by") > 0
        && oid(&merge_base))
    .then_some(merge_base)
}
pub(super) fn github_comment(
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
pub(super) fn gitlab_comment(
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
pub(super) fn gitea_comment(
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
