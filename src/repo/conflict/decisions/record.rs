//! A conflict decision: how a conflict was resolved, why, and the test that checks it; the
//! conflict it is for (by its two sides), and whether the code around it is still as it was.

use serde::{Deserialize, Serialize};

use super::super::Segment;

/// How many lines on each side of a conflict are kept to tell whether the code around it
/// changed.
const CONTEXT_LINES: usize = 3;

/// The most output of a test kept with its decision.
const MAX_OUTPUT: usize = 2000;

/// How a conflict was resolved and why, as stored.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Decision {
    /// The file it was made in.
    pub path: String,
    /// Why it was resolved so.
    #[serde(default)]
    pub note: String,
    /// A command that checks it (run in the repository's top folder), or empty.
    #[serde(default)]
    pub test: String,
    /// The text that replaced the conflict.
    pub resolution: String,
    /// The conflict's two sides, as they were.
    pub ours: String,
    pub theirs: String,
    /// The lines just before and after the conflict, as they were.
    #[serde(default)]
    pub before: String,
    #[serde(default)]
    pub after: String,
    /// Who made it, and when (Unix seconds).
    #[serde(default)]
    pub by: String,
    #[serde(default)]
    pub at: i64,
    /// How its test went the last time it ran.
    #[serde(default)]
    pub last_test: Option<TestRun>,
}

/// How a decision's test went.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestRun {
    pub passed: bool,
    /// Unix seconds.
    pub at: i64,
    /// The end of what it printed.
    #[serde(default)]
    pub output: String,
}

/// The end of `text`: at most [`MAX_OUTPUT`] bytes, from a line start when it can be.
pub fn output_tail(text: &str) -> String {
    let text = text.trim_end();
    if text.len() <= MAX_OUTPUT {
        return text.to_owned();
    }
    let mut from = text.len() - MAX_OUTPUT;
    while !text.is_char_boundary(from) {
        from += 1;
    }
    let tail = &text[from..];
    tail.split_once('\n')
        .map_or(tail, |(_, rest)| rest)
        .to_owned()
}

/// Line ends as `\n`: a file checked out with CRLF elsewhere is the same conflict.
fn normalized(text: &str) -> String {
    text.replace("\r\n", "\n")
}

/// The name a conflict is stored under: its two sides, in either order (a rebase swaps them),
/// line ends aside.
pub fn key(ours: &str, theirs: &str) -> String {
    let (a, b) = (normalized(ours), normalized(theirs));
    let (first, second) = if a <= b { (a, b) } else { (b, a) };
    let mut both = first.into_bytes();
    both.push(0);
    both.extend(second.into_bytes());
    format!("{:x}", md5::compute(both))
}

/// One conflict of the open file, as decisions are matched against it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Spot {
    pub key: String,
    pub ours: String,
    pub theirs: String,
    pub before: String,
    pub after: String,
}

/// The last `n` lines of `text`.
fn last_lines(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    lines[lines.len().saturating_sub(n)..].concat()
}

/// The first `n` lines of `text`.
fn first_lines(text: &str, n: usize) -> String {
    text.split_inclusive('\n').take(n).collect()
}

/// Each conflict of `segments`, with the lines around it.
pub(in crate::repo::conflict) fn spots(segments: &[Segment]) -> Vec<Spot> {
    let mut found = Vec::new();
    for (i, seg) in segments.iter().enumerate() {
        let Segment::Conflict { ours, theirs, .. } = seg else {
            continue;
        };
        let common = |at: Option<usize>| match at.and_then(|at| segments.get(at)) {
            Some(Segment::Common(text)) => Some(text.as_str()),
            _ => None,
        };
        let before = common(i.checked_sub(1))
            .map_or_else(String::new, |t| normalized(&last_lines(t, CONTEXT_LINES)));
        let after = common(Some(i + 1))
            .map_or_else(String::new, |t| normalized(&first_lines(t, CONTEXT_LINES)));
        found.push(Spot {
            key: key(ours, theirs),
            ours: ours.clone(),
            theirs: theirs.clone(),
            before,
            after,
        });
    }
    found
}

/// Whether `decision`, stored for `spot`'s sides, was made with the code around it as it is
/// now; when it wasn't, it is stale: offered, never applied by itself.
pub fn is_fresh(decision: &Decision, spot: &Spot) -> bool {
    normalized(&decision.before) == spot.before && normalized(&decision.after) == spot.after
}

/// Whether `decision` really is for `spot`'s conflict (its sides, in either order), not one
/// whose key only happens to be the same.
pub fn is_for(decision: &Decision, spot: &Spot) -> bool {
    let (d, s) = (
        (normalized(&decision.ours), normalized(&decision.theirs)),
        (normalized(&spot.ours), normalized(&spot.theirs)),
    );
    d == s || (d.1, d.0) == s
}

#[cfg(test)]
mod tests {
    use super::super::super::parse;
    use super::*;

    const FILE: &str = "1\n2\n3\n4\n<<<<<<< HEAD\nours\n=======\ntheirs\n>>>>>>> b\n5\n6\n7\n8\n";

    #[test]
    fn a_conflict_has_the_same_key_either_way_round() {
        assert_eq!(key("a\n", "b\n"), key("b\n", "a\n"));
        assert_eq!(key("a\r\n", "b\r\n"), key("a\n", "b\n"));
        assert_ne!(key("a\n", "b\n"), key("a\n", "c\n"));
        // The separator keeps "ab" + "" apart from "a" + "b".
        assert_ne!(key("ab", ""), key("a", "b"));
        assert_eq!(key("", "").len(), 32);
    }

    #[test]
    fn spots_keep_the_lines_around_each_conflict() {
        let (segments, _, _) = parse(FILE);
        let found = spots(&segments);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].before, "2\n3\n4\n");
        assert_eq!(found[0].after, "5\n6\n7\n");
        assert_eq!(found[0].key, key("ours\n", "theirs\n"));
        // At the ends of the file, and touching another conflict: nothing on that side.
        let edge =
            "<<<<<<< HEAD\na\n=======\nb\n>>>>>>> x\n<<<<<<< HEAD\nc\n=======\nd\n>>>>>>> x\n";
        let (segments, _, _) = parse(edge);
        let found = spots(&segments);
        assert_eq!(found.len(), 2);
        assert!(
            found
                .iter()
                .all(|s| s.before.is_empty() && s.after.is_empty())
        );
    }

    #[test]
    fn a_decision_goes_stale_when_the_code_around_it_changes() {
        let (segments, _, _) = parse(FILE);
        let spot = &spots(&segments)[0];
        let decision = Decision {
            ours: "theirs\n".into(),
            theirs: "ours\n".into(),
            before: "2\r\n3\r\n4\r\n".into(),
            after: "5\n6\n7\n".into(),
            ..Decision::default()
        };
        assert!(is_for(&decision, spot));
        assert!(is_fresh(&decision, spot));
        let moved = FILE.replace("3\n", "three\n");
        let (segments, _, _) = parse(&moved);
        let spot = &spots(&segments)[0];
        assert!(is_for(&decision, spot));
        assert!(!is_fresh(&decision, spot));
        let other = Decision {
            ours: "x\n".into(),
            ..decision
        };
        assert!(!is_for(&other, spot));
    }

    #[test]
    fn output_keeps_its_end() {
        assert_eq!(output_tail("ok\n"), "ok");
        let long = format!("{}\nlast line ü\n", "é".repeat(3000));
        let tail = output_tail(&long);
        assert!(tail.len() <= MAX_OUTPUT);
        assert!(tail.ends_with("last line ü"));
        assert_eq!(output_tail(""), "");
    }

    #[test]
    fn a_stored_decision_reads_back_and_tolerates_missing_fields() -> Result<(), String> {
        let d = Decision {
            path: "dír/ü.rs".into(),
            note: "keep the new validation, keep the old wire format".into(),
            test: "cargo test wire".into(),
            resolution: "x\n".into(),
            last_test: Some(TestRun {
                passed: true,
                at: 5,
                output: "ok".into(),
            }),
            ..Decision::default()
        };
        let json = serde_json::to_string(&d).map_err(|e| e.to_string())?;
        let back: Decision = serde_json::from_str(&json).map_err(|e| e.to_string())?;
        assert_eq!(back, d);
        let minimal: Decision = serde_json::from_str(
            r#"{"path":"p","resolution":"r","ours":"a","theirs":"b","future":1}"#,
        )
        .map_err(|e| e.to_string())?;
        assert_eq!(minimal.note, "");
        assert_eq!(minimal.last_test, None);
        Ok(())
    }
}
