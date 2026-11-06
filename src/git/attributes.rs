//! What `.gitattributes` says about files that are better read folded: `linguist-generated`
//! and `-diff`, read with `git check-attr` (so every `.gitattributes`, `info/attributes` and
//! `core.attributesFile` count, as they do for git itself).

use std::collections::HashMap;
use std::path::Path;

use super::run;

/// What the attributes say about folding one file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FoldAttr {
    /// `linguist-generated` (or `linguist-generated=true`).
    Generated,
    /// `-diff`: git shows no lines for it.
    NoDiff,
    /// `linguist-generated=false`: never folded, whatever its name.
    NotGenerated,
    /// Nothing either way.
    Unspecified,
}

/// How many bytes of paths go in one `git check-attr`: well within the length of a command
/// line on Windows (32 767 characters), and of what a remote shell is sent at once.
const CHUNK_BYTES: usize = 16 * 1024;

/// What the attributes of `paths` (relative to the repository) say about folding them, and
/// the error that left some of them unread. Paths are asked in a few commands rather than one
/// per file; between two, `go_on` says whether the answer is still wanted.
pub fn fold_attributes(
    repo: &Path,
    paths: &[String],
    go_on: &dyn Fn() -> bool,
) -> (HashMap<String, FoldAttr>, Option<String>) {
    let mut found = HashMap::with_capacity(paths.len());
    for chunk in chunks(paths, CHUNK_BYTES) {
        if !go_on() {
            break;
        }
        let mut args = vec!["check-attr", "-z", "linguist-generated", "diff", "--"];
        args.extend(chunk.iter().map(String::as_str));
        match run(Some(repo), &args) {
            Ok(out) => found.extend(parse_check_attr(&out)),
            Err(e) => return (found, Some(format!("Couldn't read .gitattributes: {e}"))),
        }
    }
    (found, None)
}

/// `paths` in runs of at most `max` bytes (a longer path alone).
fn chunks(paths: &[String], max: usize) -> Vec<&[String]> {
    let mut out = Vec::new();
    let (mut start, mut size) = (0, 0);
    for (i, path) in paths.iter().enumerate() {
        if i > start && size + path.len() > max {
            out.push(&paths[start..i]);
            (start, size) = (i, 0);
        }
        size += path.len() + 1;
    }
    if start < paths.len() {
        out.push(&paths[start..]);
    }
    out
}

/// `check-attr -z` output: `path\0attribute\0value\0`, for each path and attribute asked.
fn parse_check_attr(out: &str) -> HashMap<String, FoldAttr> {
    let mut found: HashMap<String, FoldAttr> = HashMap::new();
    let fields: Vec<&str> = out.split('\0').collect();
    let (triples, _) = fields.as_chunks::<3>();
    for &[path, attr, value] in triples {
        let said = match (attr, value) {
            ("linguist-generated", "set" | "true") => FoldAttr::Generated,
            ("linguist-generated", "unset" | "false") => FoldAttr::NotGenerated,
            ("diff", "unset") => FoldAttr::NoDiff,
            _ => FoldAttr::Unspecified,
        };
        let entry = found
            .entry(path.to_owned())
            .or_insert(FoldAttr::Unspecified);
        // `linguist-generated` is the more telling; `linguist-generated=false` wins over all.
        *entry = match (*entry, said) {
            (FoldAttr::NotGenerated, _) | (_, FoldAttr::NotGenerated) => FoldAttr::NotGenerated,
            (FoldAttr::Generated, _) | (_, FoldAttr::Generated) => FoldAttr::Generated,
            (FoldAttr::NoDiff, _) | (_, FoldAttr::NoDiff) => FoldAttr::NoDiff,
            _ => FoldAttr::Unspecified,
        };
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_attr_output_is_read() {
        let out = "a.rs\0linguist-generated\0set\0a.rs\0diff\0unspecified\0\
                   b c.lock\0linguist-generated\0unspecified\0b c.lock\0diff\0unset\0\
                   gen.rs\0linguist-generated\0false\0gen.rs\0diff\0unset\0\
                   ü.txt\0linguist-generated\0unspecified\0ü.txt\0diff\0set\0\
                   t.rs\0linguist-generated\0true\0t.rs\0diff\0unspecified\0";
        let found = parse_check_attr(out);
        assert_eq!(found["a.rs"], FoldAttr::Generated);
        assert_eq!(found["b c.lock"], FoldAttr::NoDiff);
        assert_eq!(found["gen.rs"], FoldAttr::NotGenerated);
        assert_eq!(found["ü.txt"], FoldAttr::Unspecified);
        assert_eq!(found["t.rs"], FoldAttr::Generated);
        assert!(parse_check_attr("").is_empty());
    }

    #[test]
    fn paths_are_asked_in_runs_of_a_bounded_size() {
        let paths: Vec<String> = (0..10).map(|i| format!("file{i}")).collect();
        let runs = chunks(&paths, 12);
        assert!(runs.iter().all(|r| !r.is_empty()));
        assert_eq!(runs.iter().map(|r| r.len()).sum::<usize>(), 10);
        assert!(runs.len() > 1);
        let long = vec!["x".repeat(100), "y".to_owned()];
        assert_eq!(chunks(&long, 10).len(), 2);
        assert!(chunks(&[], 10).is_empty());
    }
}
