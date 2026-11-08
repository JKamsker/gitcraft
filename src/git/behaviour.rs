use super::command::bounded;
use std::path::Path;

#[derive(Clone, Debug)]
pub(crate) struct BehaviourCommit {
    pub hash: String,
    pub tree: String,
    pub summary: String,
}
pub(crate) fn behaviour_commits(
    repo: &Path,
    revision: &str,
    count: usize,
) -> Result<Vec<BehaviourCommit>, String> {
    if revision.trim().is_empty() || revision.len() > 4096 || !(1..=32).contains(&count) {
        return Err(
            crate::i18n::text("backend-choose-an-ending-revision-and-1-32-frames-478aa2").into(),
        );
    }
    let no_lazy = super::bundles::no_lazy_fetch(repo)?;
    let revision = format!("{}^{{commit}}", revision.trim());
    let read = |args: &[&str]| {
        let words: Vec<_> = no_lazy
            .iter()
            .copied()
            .chain(args.iter().copied())
            .collect();
        bounded::run(repo, &words, vec![], 4 << 20)
    };
    let tip = String::from_utf8(read(&[
        "rev-parse",
        "--verify",
        "--end-of-options",
        &revision,
    ])?)
    .map_err(|_| crate::i18n::text("backend-commit-identity-is-not-utf-8-06a85e"))?;
    let tip = tip.trim();
    if !valid_oid(tip) {
        return Err(crate::i18n::text("backend-invalid-ending-commit-identity-60e487").into());
    }
    let count = format!("--max-count={count}");
    let bytes = read(&[
        "log",
        "--first-parent",
        "--reverse",
        &count,
        "--format=%H%x09%T%x09%s",
        tip,
        "--",
    ])?;
    parse(&bytes)
}
fn valid_oid(s: &str) -> bool {
    matches!(s.len(), 40 | 64) && s.bytes().all(|b| b.is_ascii_hexdigit())
}
fn parse(bytes: &[u8]) -> Result<Vec<BehaviourCommit>, String> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| crate::i18n::text("backend-commit-metadata-is-not-utf-8-4e5bf7"))?;
    let mut frames = Vec::new();
    for line in text.lines() {
        let words: Vec<_> = line.splitn(3, '\t').collect();
        let [hash, tree, summary] = words.as_slice() else {
            return Err(crate::i18n::text("backend-malformed-timeline-metadata-691ee5").into());
        };
        if !valid_oid(hash) || !valid_oid(tree) || hash.len() != tree.len() || frames.len() >= 32 {
            return Err(crate::i18n::text("backend-invalid-timeline-identity-count-9bb4bd").into());
        }
        frames.push(BehaviourCommit {
            hash: (*hash).into(),
            tree: (*tree).into(),
            summary: summary.chars().take(512).collect(),
        });
    }
    if frames.is_empty() {
        return Err(crate::i18n::text("backend-no-timeline-commits-found-8ca124").into());
    }
    Ok(frames)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recording_timeline_metadata_requires_pinned_complete_identities() -> Result<(), String> {
        let one = format!("{}\t{}\tfirst\n", "a".repeat(40), "b".repeat(40));
        let frames = parse(one.as_bytes()).map_err(|error| error.to_string())?;
        assert_eq!(frames[0].summary, "first");
        assert_eq!(frames[0].tree, "b".repeat(40));
        assert!(parse(b"short\tshort\tsubject").is_err());
        assert!(parse(one.repeat(33).as_bytes()).is_err());
        assert!(parse(b"").is_err());
        Ok(())
    }
}
