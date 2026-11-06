//! The issues assigned to the person `gh` is signed in as, to start work from.

use std::path::Path;

use serde_json::Value;

use super::{TOOL, gh};
use crate::forge::Issue;
use crate::pull_request::{at, items};

/// The open issues assigned to `@me`: the account `gh` is signed in to for this host.
pub(super) fn assigned(repo: &Path) -> Result<Vec<Issue>, String> {
    let out = gh(
        repo,
        &[
            "issue",
            "list",
            "--assignee",
            "@me",
            "--state",
            "open",
            "--limit",
            "100",
            "--json",
            "number,title,body,url,labels",
        ],
    )?;
    parse(&out)
}

fn parse(json: &str) -> Result<Vec<Issue>, String> {
    let value: Value =
        serde_json::from_str(json).map_err(|e| format!("unexpected {TOOL} output: {e}"))?;
    let list = value
        .as_array()
        .ok_or_else(|| format!("unexpected {TOOL} output: not a list of issues"))?;
    Ok(list
        .iter()
        .filter_map(|item| {
            Some(Issue {
                number: item.get("number").and_then(Value::as_u64)?,
                title: at(item, "/title").to_owned(),
                body: at(item, "/body").to_owned(),
                url: at(item, "/url").to_owned(),
                labels: items(item, "/labels")
                    .iter()
                    .map(|label| at(label, "/name").to_owned())
                    .filter(|name| !name.is_empty())
                    .collect(),
            })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_issues_gh_lists() {
        let json = r#"[
            {"number": 42, "title": "Fix login crash", "body": "- [ ] no crash",
             "url": "https://github.com/o/r/issues/42",
             "labels": [{"id": "x", "name": "bug", "color": "d73a4a"}, {"name": ""}]},
            {"number": 7, "title": "No body", "body": "", "url": "u", "labels": []}
        ]"#;
        let issues = parse(json).unwrap_or_default();
        assert_eq!(issues.len(), 2);
        assert_eq!(
            issues[0],
            Issue {
                number: 42,
                title: "Fix login crash".into(),
                body: "- [ ] no crash".into(),
                url: "https://github.com/o/r/issues/42".into(),
                labels: vec!["bug".into()],
            }
        );
        assert!(issues[1].body.is_empty() && issues[1].labels.is_empty());
    }

    #[test]
    fn an_issue_without_a_number_is_left_out_and_nonsense_is_an_error() {
        let issues = parse(r#"[{"title": "?"}, {"number": 1, "title": "a", "body": null}]"#);
        assert_eq!(issues.map(|l| l.len()), Ok(1));
        assert_eq!(parse("[]"), Ok(Vec::new()));
        assert!(parse("oops").is_err());
        assert!(parse(r#"{"message": "x"}"#).is_err());
    }
}
