//! Telling apart, in a file whose conflicts were resolved, the text that stands in place of each
//! conflict: the text around the conflicts is found again, in order, and what lies between is
//! that conflict's.

use super::Segment;

/// The text standing in place of each conflict of `segments` in `resolved`, in order; `None`
/// when the text around them isn't found again as it was (edited too), or two conflicts touch
/// (nothing tells where one ends and the next begins).
pub(super) fn split_resolutions(segments: &[Segment], resolved: &str) -> Option<Vec<String>> {
    let mut texts = Vec::new();
    // Where the text not yet accounted for starts, and whether a conflict is waiting for the
    // common text that ends it.
    let mut pos = 0;
    let mut open = false;
    let last = segments.len().saturating_sub(1);
    for (i, seg) in segments.iter().enumerate() {
        match seg {
            Segment::Conflict { .. } => {
                if open {
                    return None;
                }
                open = true;
            }
            Segment::Common(common) => {
                let rest = resolved.get(pos..)?;
                let at = if !open {
                    // Before any conflict: the file starts with it.
                    rest.starts_with(common.as_str()).then_some(0)?
                } else if i == last {
                    // After the last one: the file ends with it.
                    rest.len()
                        .checked_sub(common.len())
                        .filter(|&at| rest.get(at..) == Some(common.as_str()))?
                } else {
                    find_line_start(rest, common)?
                };
                if open {
                    texts.push(rest.get(..at)?.to_owned());
                    open = false;
                }
                pos += at + common.len();
            }
        }
    }
    if open {
        texts.push(resolved.get(pos..)?.to_owned());
    } else if pos != resolved.len() {
        return None;
    }
    Some(texts)
}

/// Where `needle` occurs in `hay` at the start of a line (the text around a conflict starts on
/// a line of its own), if it does so once only: found twice, either could end the conflict
/// before it, and taking the wrong one would give a conflict lines of the next one.
fn find_line_start(hay: &str, needle: &str) -> Option<usize> {
    let mut found = None;
    let mut from = 0;
    while let Some(next) = hay.get(from..).and_then(|rest| rest.find(needle)) {
        let at = from + next;
        if at == 0 || hay.as_bytes().get(at - 1) == Some(&b'\n') {
            if found.is_some() {
                return None;
            }
            found = Some(at);
        }
        from = at + hay.get(at..)?.chars().next()?.len_utf8();
    }
    found
}

#[cfg(test)]
mod tests {
    use super::super::parse;
    use super::*;

    fn split(marked: &str, resolved: &str) -> Option<Vec<String>> {
        let (segments, _, _) = parse(marked);
        split_resolutions(&segments, resolved)
    }

    #[test]
    fn finds_each_conflicts_text() {
        let marked = "a\n<<<<<<< HEAD\nx\n=======\ny\n>>>>>>> b\nmid\n<<<<<<< HEAD\n1\n=======\n2\n>>>>>>> b\nz\n";
        assert_eq!(
            split(marked, "a\nxy\nmid\n\nz\n"),
            Some(vec!["xy\n".to_owned(), "\n".to_owned()])
        );
        // A conflict resolved to nothing.
        assert_eq!(
            split(marked, "a\nmid\n2\nz\n"),
            Some(vec![String::new(), "2\n".to_owned()])
        );
    }

    #[test]
    fn conflicts_at_the_ends_of_the_file() {
        let marked =
            "<<<<<<< HEAD\nx\n=======\ny\n>>>>>>> b\nmid\n<<<<<<< HEAD\n1\n=======\n2\n>>>>>>> b\n";
        assert_eq!(
            split(marked, "both\nmid\nlast\n"),
            Some(vec!["both\n".to_owned(), "last\n".to_owned()])
        );
    }

    #[test]
    fn the_text_around_edited_is_not_split() {
        let marked = "a\n<<<<<<< HEAD\nx\n=======\ny\n>>>>>>> b\nz\n";
        assert_eq!(split(marked, "A\nx\nz\n"), None);
        assert_eq!(split(marked, "a\nx\nZ\n"), None);
    }

    #[test]
    fn touching_conflicts_are_not_split() {
        let marked =
            "<<<<<<< HEAD\nx\n=======\ny\n>>>>>>> b\n<<<<<<< HEAD\n1\n=======\n2\n>>>>>>> b\n";
        assert_eq!(split(marked, "x\n1\n"), None);
    }

    /// The text between two conflicts may also be in the first one's resolution: it is found
    /// at the start of a line, the first time.
    #[test]
    fn repeated_text_and_unicode() {
        let marked = "é\n<<<<<<< HEAD\nü\n=======\nß\n>>>>>>> b\n名前\n<<<<<<< HEAD\n1\n=======\n2\n>>>>>>> b\n終\n";
        assert_eq!(
            split(marked, "é\nü x名前\n名前\n2\n終\n"),
            Some(vec!["ü x名前\n".to_owned(), "2\n".to_owned()])
        );
        assert_eq!(split("", ""), Some(vec![]));
    }

    /// The text between two conflicts found twice: which conflict the lines between belong to
    /// can't be told, so the file isn't split.
    #[test]
    fn ambiguous_text_between_conflicts_is_not_split() {
        let marked = "<<<<<<< HEAD\nX1o\n=======\nX1t\n>>>>>>> b\n}\n<<<<<<< HEAD\nY1o\n=======\nY1t\n>>>>>>> b\nend\n";
        assert_eq!(split(marked, "X1o\n}\nfn new() {\n}\nY1t\nend\n"), None);
        assert_eq!(
            split(marked, "X1o\n}\nY1t\nend\n"),
            Some(vec!["X1o\n".to_owned(), "Y1t\n".to_owned()])
        );
    }

    #[test]
    fn crlf_lines() {
        let marked = "a\r\n<<<<<<< HEAD\r\nx\r\n=======\r\ny\r\n>>>>>>> b\r\nz\r\n";
        assert_eq!(
            split(marked, "a\r\nxy\r\nz\r\n"),
            Some(vec!["xy\r\n".to_owned()])
        );
    }
}
