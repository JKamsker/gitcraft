//! Immutable release ranges for composed postcards. Read only, mailmap aware, never fetches objects.
use super::activity::process;
pub(crate) fn oid(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|b| b.is_ascii_hexdigit())
}
use crate::git::Commit;
use std::path::Path;
pub(crate) const LIMIT: usize = 200;
pub(crate) fn range(
    repo: &Path,
    base: &str,
    target: &str,
    active: &dyn Fn() -> bool,
) -> Result<Vec<Commit>, String> {
    if !oid(target) || (!base.is_empty() && !oid(base)) {
        return Err(crate::i18n::text(
            "backend-choose-immutable-commit-ids-for-both-range-ends-ebe621",
        )
        .into());
    }
    let mut input = format!("{target}\n");
    if !base.is_empty() {
        input.push_str(&format!("^{base}\n"));
    }
    let output = process::run(
        repo,
        &[
            "log",
            "--no-show-signature",
            "--encoding=UTF-8",
            "--topo-order",
            "--max-count=201",
            "--date=short",
            "--format=%H%x00%P%x00%aN%x00%ad%x00%s%x00",
            "--stdin",
        ],
        Some(input.as_bytes()),
        active,
    )?;
    parse(&output)
}
fn parse(output: &[u8]) -> Result<Vec<Commit>, String> {
    let text = std::str::from_utf8(output)
        .map_err(|_| crate::i18n::text("backend-release-history-is-not-utf-8-8f357f"))?;
    let fields: Vec<_> = text.split('\0').collect();
    let (records, tail) = fields.as_chunks::<5>();
    if !tail.iter().all(|f| f.trim().is_empty()) {
        return Err(crate::i18n::text(
            "backend-release-history-returned-incomplete-metadata-669952",
        )
        .into());
    }
    if records.len() > LIMIT {
        return Err(crate::i18n::text(
            "backend-this-range-exceeds-the-postcard-s-200-commit-limit-choose-a-narro-6ed720",
        )
        .into());
    }
    let commits: Result<Vec<_>, String> = records
        .iter()
        .map(|[hash, parents, author, date, summary]| {
            let hash = hash.trim();
            if !oid(hash) || !parents.split_whitespace().all(oid) {
                return Err(crate::i18n::text(
                    "backend-release-history-returned-invalid-commit-ids-2acae8",
                )
                .into());
            }
            Ok(Commit {
                hash: hash.into(),
                parents: parents.split_whitespace().map(str::to_owned).collect(),
                author: (*author).into(),
                date: (*date).into(),
                summary: (*summary).into(),
                ..Default::default()
            })
        })
        .collect();
    let commits = commits?;
    if commits.is_empty() {
        return Err(crate::i18n::text(
            "backend-the-selected-release-range-contains-no-commits-1e8d7d",
        )
        .into());
    }
    Ok(commits)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn metadata_parser_bounds_release_size_and_rejects_incomplete_records() {
        let item = format!("{}\0\0Canonical\02026-11-07\0Release\0\n", "a".repeat(40));
        assert_eq!(parse(item.as_bytes()).map(|c| c.len()), Ok(1));
        assert!(parse(item.repeat(LIMIT + 1).as_bytes()).is_err());
        assert!(parse(b"abc\0truncated").is_err());
    }
}
