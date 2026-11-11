//! Search.

use super::*;

/// Commits whose message or author matches `query` (case-insensitive), or whose hash starts
/// with it, newest first; among those of the revisions `from` when given, read in `order`, as
/// `log`.
pub fn search(
    repo: &Path,
    from: Option<&[String]>,
    order: LogOrder,
    query: &str,
) -> Result<Vec<String>, String> {
    if from.is_some_and(|revs| revs.is_empty()) {
        return Ok(Vec::new());
    }
    let mut hashes: Vec<String> = Vec::new();
    // A short query matches most of history: a linear check per hash would be quadratic.
    let mut seen: HashSet<String> = HashSet::new();
    let mut base = vec!["log", NO_SIGNATURE, UTF8];
    base.extend(order.args());
    base.extend(["-i", "--format=%H"]);
    for filter in [format!("--grep={query}"), format!("--author={query}")] {
        let mut args = base.clone();
        args.push("--fixed-strings");
        args.push(&filter);
        push_revisions(&mut args, from);
        if let Ok(out) = run(Some(repo), &args) {
            for h in out.lines() {
                if seen.insert(h.to_owned()) {
                    hashes.push(h.to_owned());
                }
            }
        }
    }
    let q = query.trim();
    if q.len() >= 4
        && q.chars().all(|c| c.is_ascii_hexdigit())
        && let Ok(h) = run(
            Some(repo),
            &["rev-parse", "--verify", "-q", &format!("{q}^{{commit}}")],
        )
    {
        // First, and once: a tag `2024` may be on a commit whose message has `2024` in it.
        let h = h.trim();
        hashes.retain(|x| x != h);
        hashes.insert(0, h.to_owned());
    }
    Ok(hashes)
}

/// Commits that added or removed `query` in a file's contents (`git log -S`), newest first;
/// among those of the revisions `from` when given, read in `order`, as `log`.
pub fn search_code(
    repo: &Path,
    from: Option<&[String]>,
    order: LogOrder,
    query: &str,
) -> Result<Vec<String>, String> {
    if from.is_some_and(|revs| revs.is_empty()) {
        return Ok(Vec::new());
    }
    let pickaxe = format!("-S{query}");
    let mut args = vec!["log", NO_SIGNATURE];
    args.extend(order.args());
    args.extend(["--format=%H", pickaxe.as_str()]);
    push_revisions(&mut args, from);
    let out = run(Some(repo), &args)?;
    Ok(out.lines().map(str::to_owned).collect())
}

/// The relation view's explicit older-context read; reuse the ordinary log format/parser.
pub(crate) fn investigation_page(
    reader: &super::super::investigations::Reader<'_>,
    tips: &[String],
    order: LogOrder,
    skip: usize,
    limit: usize,
) -> Result<Vec<Commit>, String> {
    let max = format!("--max-count={limit}");
    let args = log_args(Some(tips), order, skip, Some(&max), &[]);
    let output = reader.run(
        &args
            .as_args()
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        Vec::new(),
        &[0],
    )?;
    parse_investigation_commits(&output)
}

pub(crate) fn investigation_commits_of(
    reader: &super::super::investigations::Reader<'_>,
    hashes: &[String],
) -> Result<Vec<Commit>, String> {
    if hashes.len() > super::super::LOG_PAGE
        || hashes.iter().any(|s| !super::super::investigations::oid(s))
    {
        return Err(crate::i18n::text("backend-invalid-pinned-commit-request-ed2691").into());
    }
    if hashes.is_empty() {
        return Ok(Vec::new());
    }
    let format = log_args(None, LogOrder::default(), 0, None, &[]).format;
    let mut result = Vec::new();
    for chunk in hashes.chunks(100) {
        let mut args = vec![
            "log",
            NO_SIGNATURE,
            UTF8,
            "--no-walk=unsorted",
            "--date=format-local:%Y-%m-%d %H:%M",
            &format,
            END_OPTS,
        ];
        args.extend(chunk.iter().map(String::as_str));
        args.push("--");
        result.extend(parse_investigation_commits(&reader.run(
            &args,
            vec![],
            &[0],
        )?)?);
    }
    Ok(result)
}

pub(super) fn parse_investigation_commits(output: &[u8]) -> Result<Vec<Commit>, String> {
    let text = std::str::from_utf8(output).map_err(|_| {
        crate::i18n::text("backend-history-page-returned-non-utf-8-metadata-333f22")
    })?;
    let fields: Vec<_> = text.split('\0').collect();
    let (records, tail) = fields.as_chunks::<LOG_FIELDS>();
    if !tail.iter().all(|s| s.trim().is_empty()) {
        return Err(
            crate::i18n::text("backend-history-page-returned-truncated-metadata-cc11d9").into(),
        );
    }
    records
        .iter()
        .map(|record| {
            parse_record(record)
                .filter(|c| {
                    super::super::investigations::oid(&c.hash)
                        && c.parents
                            .iter()
                            .all(|p| super::super::investigations::oid(p))
                })
                .ok_or_else(|| {
                    crate::i18n::text(
                        "backend-history-page-returned-invalid-commit-identity-8d0985",
                    )
                    .into()
                })
        })
        .collect()
}
