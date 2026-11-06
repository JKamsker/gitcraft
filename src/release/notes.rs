//! Release notes from the changes since the last release: grouped by type, with links to the
//! pull requests.

use super::conventional::Change;

/// The groups the notes are made of, in order: (title, the types in it).
const GROUPS: [(&str, &[&str]); 5] = [
    ("Features", &["feat"]),
    ("Bug fixes", &["fix"]),
    ("Performance", &["perf"]),
    ("Documentation", &["docs"]),
    ("Other changes", &[]),
];

/// The notes for `changes`, newest first as `git log` lists them. `link` makes the URL of a
/// pull request from its number (none: `#12` stays as it is).
pub(crate) fn notes(changes: &[Change], link: &dyn Fn(u64) -> Option<String>) -> String {
    let changes: Vec<&Change> = changes.iter().filter(|c| !c.is_release()).collect();
    let mut out = String::new();
    let breaking: Vec<&&Change> = changes.iter().filter(|c| c.breaking).collect();
    if !breaking.is_empty() {
        out.push_str("### Breaking changes\n\n");
        for c in breaking {
            out.push_str(&line(c, link));
        }
        out.push('\n');
    }
    for (title, kinds) in GROUPS {
        let known = |c: &&&Change| GROUPS.iter().any(|(_, k)| k.contains(&c.kind.as_str()));
        // A breaking change is listed under Breaking changes only.
        let in_group: Vec<&&Change> = changes
            .iter()
            .filter(|c| !c.breaking)
            .filter(|c| {
                if kinds.is_empty() {
                    !known(c)
                } else {
                    kinds.contains(&c.kind.as_str())
                }
            })
            .collect();
        if in_group.is_empty() {
            continue;
        }
        out.push_str(&format!("### {title}\n\n"));
        for c in in_group {
            out.push_str(&line(c, link));
        }
        out.push('\n');
    }
    out.trim_end().to_owned()
}

/// `- **scope:** summary ([#12](url))`, its own `(#12)` taken out of the summary.
fn line(c: &Change, link: &dyn Fn(u64) -> Option<String>) -> String {
    let mut summary = c.summary.clone();
    for n in &c.prs {
        summary = summary.replace(&format!(" (#{n})"), "");
    }
    let mut text = String::from("- ");
    if !c.scope.is_empty() {
        text.push_str(&format!("**{}:** ", c.scope));
    }
    text.push_str(summary.trim());
    let refs: Vec<String> = c
        .prs
        .iter()
        .map(|n| match link(*n) {
            Some(url) => format!("[#{n}]({url})"),
            None => format!("#{n}"),
        })
        .collect();
    if refs.is_empty() {
        text.push_str(&format!(" ({})", crate::repo::short(&c.hash)));
    } else {
        text.push_str(&format!(" ({})", refs.join(", ")));
    }
    text.push('\n');
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_are_grouped_and_linked() {
        let changes = [
            Change::parse("aaaaaaaaaa", "feat(ui): dark mode (#12)", ""),
            Change::parse("bbbbbbbbbb", "fix: crash on ü", ""),
            Change::parse("cccccccccc", "refactor!: drop the old API", ""),
            Change::parse("dddddddddd", "chore(release): v1.0.0", ""),
            Change::parse("eeeeeeeeee", "Update readme", ""),
        ];
        let link = |n: u64| Some(format!("https://x/pull/{n}"));
        let text = notes(&changes, &link);
        assert_eq!(
            text,
            "### Breaking changes\n\n- drop the old API (cccccccc)\n\n\
             ### Features\n\n- **ui:** dark mode ([#12](https://x/pull/12))\n\n\
             ### Bug fixes\n\n- crash on ü (bbbbbbbb)\n\n\
             ### Other changes\n\n- Update readme (eeeeeeee)"
        );
        assert_eq!(notes(&[], &link), "");
        let unlinked = notes(&changes[..1], &|_| None);
        assert!(unlinked.contains("dark mode (#12)"));
    }
}
