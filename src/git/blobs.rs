//! Files as they are at revisions, many in one read: `git cat-file --batch` given `rev:path`
//! names, so that a view reading a hundred files at a commit starts git once, not a hundred
//! times, and never looks at the working tree.

use std::path::Path;

use super::run_captured;

/// The text of each `(rev, path)` asked for, in order: `None` for one that isn't there (no
/// such file at that revision), or isn't a file, or is bigger than `max` bytes. Invalid UTF-8
/// is replaced.
pub fn read(
    repo: &Path,
    files: &[(String, String)],
    max: usize,
) -> Result<Vec<Option<String>>, String> {
    if files.is_empty() {
        return Ok(Vec::new());
    }
    let mut input = String::new();
    for (rev, path) in files {
        // A name with a line break can't be asked for on one line: it is answered missing.
        if path.contains('\n') || rev.contains('\n') {
            input.push_str("-\n");
        } else {
            input.push_str(&format!("{rev}:{path}\n"));
        }
    }
    let out = run_captured(repo, &["cat-file", "--batch"], Some(input.as_bytes()))?;
    if out.code != Some(0) {
        return Err(out.stderr.trim().to_owned());
    }
    Ok(parse(&out.raw, files.len(), max))
}

/// What `cat-file --batch` printed for `count` names.
pub fn parse(raw: &[u8], count: usize, max: usize) -> Vec<Option<String>> {
    let mut found = Vec::with_capacity(count);
    let mut at = 0;
    while found.len() < count && at < raw.len() {
        let Some(end) = raw[at..].iter().position(|&b| b == b'\n').map(|i| at + i) else {
            break;
        };
        let header = String::from_utf8_lossy(&raw[at..end]);
        at = end + 1;
        let mut parts = header.split(' ');
        let (_, kind, size) = (parts.next(), parts.next(), parts.next());
        let Some(size) = size.and_then(|s| s.parse::<usize>().ok()) else {
            // `<name> missing` (or `ambiguous`): nothing follows.
            found.push(None);
            continue;
        };
        let body_end = (at + size).min(raw.len());
        let body = &raw[at..body_end];
        at = body_end + 1;
        found.push(
            (kind == Some("blob") && size <= max)
                .then(|| String::from_utf8_lossy(body).into_owned()),
        );
    }
    found.resize(count, None);
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_batch_is_read_back_in_order_with_the_missing_ones() {
        let raw = b"abc blob 3\nx\ny\nmain:gone missing\ndef tree 2\nzz\nfff blob 5\nhello\n";
        let got = parse(raw, 4, 4);
        assert_eq!(got, [Some("x\ny".to_owned()), None, None, None]);
        assert_eq!(parse(raw, 4, 100)[3].as_deref(), Some("hello"));
    }

    #[test]
    fn files_are_read_at_their_revision_not_from_the_working_tree() -> Result<(), String> {
        let repo = crate::git::fixture::Repo::new("blobs")?;
        let one = repo.commit("one", &[("a b.rs", Some("one\n"))])?;
        repo.commit("two", &[("a b.rs", Some("two\n"))])?;
        std::fs::write(repo.path().join("a b.rs"), "dirty\n").map_err(|e| e.to_string())?;
        let got = read(
            repo.path(),
            &[
                (one.clone(), "a b.rs".to_owned()),
                ("HEAD".to_owned(), "a b.rs".to_owned()),
                (one, "nope.rs".to_owned()),
            ],
            1000,
        )?;
        assert_eq!(
            got,
            [Some("one\n".to_owned()), Some("two\n".to_owned()), None]
        );
        Ok(())
    }
}
