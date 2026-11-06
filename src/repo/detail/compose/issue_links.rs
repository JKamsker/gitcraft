//! Issue references in a commit message (`JIRA-12`, `#12`) as links: each rule's pattern
//! (a regular expression) gives the text, its URL template (`$0`, `$1`…) where it goes. The
//! forge's `#12` is linked without any rule.

use egui::{RichText, Ui};
use regex::Regex;

use super::store::IssueRule;
use crate::forge::{Forge, Hosting};
use crate::theme::Tokens;

/// The most links listed under a message.
const MAX_LINKS: usize = 12;

/// The longest pattern accepted, and how big it may compile to: a rule can't make every
/// message slow to show.
const MAX_PATTERN: usize = 500;
const SIZE_LIMIT: usize = 1 << 20;

/// A reference found in a message, and the page it opens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IssueLink {
    pub text: String,
    pub url: String,
}

/// The rule `#12` → the forge's issue 12, for a repository hosted on `hosting`.
pub fn forge_rule(hosting: &Hosting) -> IssueRule {
    let issues = match hosting.forge {
        Forge::GitLab => "-/issues",
        Forge::GitHub | Forge::Forgejo | Forge::Gitea => "issues",
    };
    IssueRule {
        pattern: r"\B#(\d+)\b".to_owned(),
        url: format!("{}/{}/{issues}/$1", hosting.web, hosting.path),
    }
}

/// `rule`'s pattern compiled, or why it can't be.
pub fn compile(rule: &IssueRule) -> Result<Regex, String> {
    let pattern = rule.pattern.trim();
    if pattern.is_empty() {
        return Err("The pattern is empty".to_owned());
    }
    if pattern.len() > MAX_PATTERN {
        return Err(format!(
            "The pattern is longer than {MAX_PATTERN} characters"
        ));
    }
    regex::RegexBuilder::new(pattern)
        .size_limit(SIZE_LIMIT)
        .build()
        .map_err(|e| match e {
            regex::Error::Syntax(s) => format!("Not a valid pattern: {}", last_line(&s)),
            regex::Error::CompiledTooBig(_) => "The pattern is too complex".to_owned(),
            other => format!("Not a valid pattern: {other}"),
        })
}

fn last_line(s: &str) -> &str {
    s.lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or(s)
        .trim()
}

/// Why `rule` can't be used, if it can't: its pattern, or a URL that isn't a web page.
pub fn check(rule: &IssueRule) -> Result<(), String> {
    compile(rule)?;
    let sample = rule.url.replace(['$', '{', '}'], "");
    if !crate::pull_request::is_web_url(sample.trim()) {
        return Err("The URL must start with https:// or http://".to_owned());
    }
    Ok(())
}

/// The links of `message` by `rules`, in the order they appear, each text once.
pub fn find(message: &str, rules: &[IssueRule]) -> Vec<IssueLink> {
    let mut found: Vec<(usize, IssueLink)> = Vec::new();
    for rule in rules {
        let Ok(re) = compile(rule) else { continue };
        for caps in re.captures_iter(message) {
            let Some(all) = caps.get(0) else { continue };
            if all.as_str().trim().is_empty() || found.iter().any(|(_, l)| l.text == all.as_str()) {
                continue;
            }
            let mut url = String::new();
            caps.expand(&rule.url, &mut url);
            let url = url.trim().to_owned();
            // Only a web page: a pattern can't make a link that runs something.
            if crate::pull_request::is_web_url(&url) {
                found.push((
                    all.start(),
                    IssueLink {
                        text: all.as_str().to_owned(),
                        url,
                    },
                ));
            }
            if found.len() >= MAX_LINKS * 4 {
                break;
            }
        }
    }
    found.sort_by_key(|(at, _)| *at);
    found.into_iter().map(|(_, l)| l).take(MAX_LINKS).collect()
}

/// The rules for a repository: the user's, then the forge's `#12` when it is hosted on one.
pub fn rules_for(own: &[IssueRule], hosting: Option<&Hosting>) -> Vec<IssueRule> {
    own.iter().cloned().chain(hosting.map(forge_rule)).collect()
}

/// A row of links for the references in `message`, if it has any.
pub fn links_row(ui: &mut Ui, message: &str, rules: &[IssueRule]) {
    // Found once per message and rules, not on every frame.
    let id = egui::Id::new(("issue-links", message, rules));
    let links = match ui.ctx().data(|d| d.get_temp::<Vec<IssueLink>>(id)) {
        Some(links) => links,
        None => {
            let links = find(message, rules);
            ui.ctx().data_mut(|d| d.insert_temp(id, links.clone()));
            links
        }
    };
    if links.is_empty() {
        return;
    }
    let t = Tokens::get(ui.ctx());
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("Issues").color(t.text_faint).small());
        for link in links {
            ui.hyperlink_to(RichText::new(&link.text).small(), &link.url)
                .on_hover_text(&link.url);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(pattern: &str, url: &str) -> IssueRule {
        IssueRule {
            pattern: pattern.into(),
            url: url.into(),
        }
    }

    fn hosting(forge: Forge) -> Hosting {
        Hosting {
            forge,
            host: "example.com".into(),
            path: "team/app".into(),
            web: "https://example.com".into(),
        }
    }

    #[test]
    fn forge_issues_are_linked() {
        let rules = rules_for(&[], Some(&hosting(Forge::GitHub)));
        let links = find("fix: crash (#12), see #7 and #12; not a#3 nor #x", &rules);
        let urls: Vec<&str> = links.iter().map(|l| l.url.as_str()).collect();
        assert_eq!(
            urls,
            [
                "https://example.com/team/app/issues/12",
                "https://example.com/team/app/issues/7"
            ]
        );
        let gitlab = rules_for(&[], Some(&hosting(Forge::GitLab)));
        assert_eq!(
            find("#5", &gitlab)[0].url,
            "https://example.com/team/app/-/issues/5"
        );
        assert!(find("#5", &rules_for(&[], None)).is_empty());
    }

    #[test]
    fn own_rules_come_with_their_groups() {
        let rules = [rule(
            r"\b([A-Z]{2,10})-(\d+)\b",
            "https://jira.example.com/browse/$1-$2",
        )];
        let links = find("feat: JIRA-12 and ÄB-3, OPS-7 — JIRA-12 again", &rules);
        let texts: Vec<&str> = links.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(texts, ["JIRA-12", "OPS-7"]);
        assert_eq!(links[1].url, "https://jira.example.com/browse/OPS-7");
        assert!(find("", &rules).is_empty());
    }

    #[test]
    fn bad_rules_are_refused_and_skipped() {
        assert!(check(&rule("(", "https://x/$0")).is_err());
        assert!(check(&rule("", "https://x/$0")).is_err());
        assert!(check(&rule(&"a".repeat(MAX_PATTERN + 1), "https://x")).is_err());
        assert!(check(&rule("x", "javascript:alert($0)")).is_err());
        assert!(check(&rule("x", "file:///etc/$0")).is_err());
        assert!(check(&rule(r"T-\d+", "https://t.example/${0}")).is_ok());
        // A URL a match turns into something that isn't a web page isn't linked.
        let sneaky = [rule(r"(\S+)", "$1")];
        assert!(find("javascript:alert(1)", &sneaky).is_empty());
        // Nor is a pattern that matches nothing in particular.
        assert!(find("abc", &[rule(r"\s*", "https://x/$0")]).is_empty());
    }

    #[test]
    fn a_huge_message_lists_a_few() {
        let message = (0..10_000).map(|i| format!("#{i} ")).collect::<String>();
        let links = find(&message, &rules_for(&[], Some(&hosting(Forge::Gitea))));
        assert_eq!(links.len(), MAX_LINKS);
        assert_eq!(links[0].text, "#0");
    }
}
