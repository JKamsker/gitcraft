//! Making a pull request's text safe to show: no images from servers its author chose.

use super::html::{img_tags_to_markdown, without_comments, without_html_tags};
use std::collections::HashSet;

use super::tables::{Block, split_image_tables};

/// `body` made safe and fit to show: without HTML comments (what templates leave behind), with
/// `<img>` tags as images, and images from anywhere but `host` as links, so that reading a pull
/// request doesn't make the app contact a server its author chose.
pub fn safe_markdown(body: &str, host: &str) -> String {
    images_to_links(&without_html(body), host)
}

/// `body` without HTML comments, with `<img>` tags as images and without other tags.
fn without_html(body: &str) -> String {
    let body = without_comments(body);
    let body = img_tags_to_markdown(&body);
    without_html_tags(&body)
}

/// `body` made safe to show like [`safe_markdown`], for a file of the repository: `local` gets
/// the address of each image and gives the one to show it from instead (a picture the app holds
/// itself), or `None` to make it a link. Gives the text and the addresses `local` gave, which
/// [`images_to_links_except`] keeps when a piece of the text is checked again on its own.
pub fn safe_local_markdown(
    body: &str,
    local: &mut dyn FnMut(&str) -> Option<String>,
) -> (String, HashSet<String>) {
    let mut kept = HashSet::new();
    let text = local_images(&without_html(body), &mut |url| {
        let uri = local(url)?;
        kept.insert(uri.clone());
        Some(uri)
    });
    let text = images_to_links_except(&text, &|url| kept.contains(url));
    (text, kept)
}

/// The images of `body` that `local` gives another address, at that address.
fn local_images(body: &str, local: &mut dyn FnMut(&str) -> Option<String>) -> String {
    use pulldown_cmark::{Event, Parser, Tag};
    let mut out = String::with_capacity(body.len());
    let mut copied = 0;
    let mut events = Parser::new_ext(body, markdown_options()).into_offset_iter();
    while let Some((event, range)) = events.next() {
        let Event::Start(Tag::Image { dest_url, .. }) = event else {
            continue;
        };
        let label = image_label(&mut events);
        let Some(uri) = local(&dest_url) else {
            continue;
        };
        let Some(before) = body.get(copied..range.start) else {
            continue;
        };
        out.push_str(before);
        out.push('!');
        out.push_str(&link_markdown(&label, &uri));
        copied = range.end;
    }
    out.push_str(&body[copied..]);
    out
}

/// The text of an image whose start was just read, read up to its end.
fn image_label<'a>(
    events: &mut impl Iterator<Item = (pulldown_cmark::Event<'a>, std::ops::Range<usize>)>,
) -> String {
    use pulldown_cmark::{Event, Tag, TagEnd};
    let mut label = String::new();
    let mut depth = 1;
    for (inner, _) in events.by_ref() {
        match inner {
            Event::Start(Tag::Image { .. }) => depth += 1,
            Event::End(TagEnd::Image) => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            Event::Text(text) | Event::Code(text) => label.push_str(&text),
            _ => {}
        }
    }
    label
}

/// `body` made safe, and cut into what the page shows one after the other.
pub fn safe_blocks(body: &str, host: &str) -> Vec<Block> {
    // A piece can read differently from the whole, say a table the whole has in a code block, so
    // each is checked for images on its own too.
    let check = |text: &str| images_to_links(text, host);
    split_image_tables(&safe_markdown(body, host))
        .into_iter()
        .map(|block| match block {
            Block::Markdown(text) => Block::Markdown(check(&text)),
            Block::ImageTable { header, rows } => Block::ImageTable {
                header: header.iter().map(|cell| check(cell)).collect(),
                rows: rows
                    .iter()
                    .map(|row| row.iter().map(|cell| check(cell)).collect())
                    .collect(),
            },
        })
        .collect()
}

/// `body` with its images from anywhere but `host` as links. The viewer's own parser finds them:
/// references, escapes and images in images are all images too, which a search of the text misses.
fn images_to_links(body: &str, host: &str) -> String {
    images_to_links_except(body, &|url| is_trusted_image(url, host))
}

/// `body` with its images as links, but for those at an address `trusted` takes.
pub fn images_to_links_except(body: &str, trusted: &dyn Fn(&str) -> bool) -> String {
    let mut text = body.to_owned();
    // A link right after a `!` reads as an image again, so go on until nothing changes.
    for _ in 0..MAX_IMAGE_PASSES {
        let next = image_pass(&text, trusted);
        if next == text {
            return text;
        }
        text = next;
    }
    // Still images after all those passes: no `![` is left to start one.
    text.replace("![", "\\![")
}

const MAX_IMAGE_PASSES: usize = 8;

fn image_pass(body: &str, trusted: &dyn Fn(&str) -> bool) -> String {
    use pulldown_cmark::{Event, Parser, Tag};
    let mut out = String::with_capacity(body.len());
    let mut copied = 0;
    let mut events = Parser::new_ext(body, markdown_options()).into_offset_iter();
    while let Some((event, range)) = events.next() {
        let Event::Start(Tag::Image { dest_url, .. }) = event else {
            continue;
        };
        if trusted(&dest_url) {
            continue;
        }
        let label = image_label(&mut events);
        let Some(before) = body.get(copied..range.start) else {
            continue;
        };
        // A `!` right before the image's own (`!![x](…)`) would make the link an image again:
        // it stays, escaped.
        match before.strip_suffix('!') {
            // Not one escaped already (after an odd number of `\`).
            Some(rest) if rest.chars().rev().take_while(|&c| c == '\\').count() % 2 == 0 => {
                out.push_str(rest);
                out.push_str("\\!");
            }
            _ => out.push_str(before),
        }
        out.push_str(&link_markdown(&label, &dest_url));
        copied = range.end;
    }
    out.push_str(&body[copied..]);
    out
}

fn link_markdown(label: &str, url: &str) -> String {
    let label = if label.trim().is_empty() {
        "image".to_owned()
    } else {
        label
            .replace('\\', "\\\\")
            .replace('[', "\\[")
            .replace(']', "\\]")
            .replace(['\n', '\r'], " ")
    };
    let plain = !url
        .chars()
        .any(|c| c.is_whitespace() || matches!(c, '(' | ')' | '<' | '>' | '\\'));
    if plain {
        format!("[{label}]({url})")
    } else {
        let url = url
            .replace('<', "%3C")
            .replace('>', "%3E")
            .replace('\\', "%5C")
            .replace(['\n', '\r'], "");
        format!("[{label}](<{url}>)")
    }
}

/// What the viewer parses Markdown with, so that this module sees the same images it does.
pub(crate) fn markdown_options() -> pulldown_cmark::Options {
    use pulldown_cmark::Options;
    Options::ENABLE_TABLES
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_DEFINITION_LIST
}

fn is_trusted_image(url: &str, host: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else {
        return false;
    };
    let image_host = rest
        .split(['/', '?', '#'])
        .next()
        .unwrap_or("")
        .to_lowercase();
    let plain_name = image_host
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-'));
    image_host == host
        || (host == "github.com" && plain_name && image_host.ends_with(".githubusercontent.com"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn images_from_the_pull_requests_host_stay() {
        let body = "![shot](https://github.com/user-attachments/assets/a)";
        assert_eq!(safe_markdown(body, "github.com"), body);
        let raw = "![shot](https://raw.githubusercontent.com/me/r/1/a.png)";
        assert_eq!(safe_markdown(raw, "github.com"), raw);
    }

    /// Every image in `text` as the viewer parses it.
    fn images_in(text: &str) -> Vec<String> {
        use pulldown_cmark::{Event, Parser, Tag};
        Parser::new_ext(text, markdown_options())
            .filter_map(|e| match e {
                Event::Start(Tag::Image { dest_url, .. }) => Some(dest_url.to_string()),
                _ => None,
            })
            .collect()
    }

    /// `!`s before an image don't make its link an image again, however many there are.
    #[test]
    fn bangs_before_an_image_leave_no_image() {
        for n in [1, 2, 9, 10, 20, 30, 64] {
            let body = format!("{}[x](https://evil.example/a.png)", "!".repeat(n));
            let safe = safe_markdown(&body, "github.com");
            assert!(images_in(&safe).is_empty(), "{n}: {safe}");
            assert!(safe.contains("https://evil.example/a.png"), "{n}: {safe}");
            let local = images_to_links_except(&body, &|_| false);
            assert!(images_in(&local).is_empty(), "{n}: {local}");
        }
        // An escaped `!` before a link is no image: left as it is.
        let escaped = "\\![x](https://evil.example/a.png)";
        assert_eq!(safe_markdown(escaped, "github.com"), escaped);
        // After an escaped `\`, the `!` starts an image: the link keeps it escaped.
        let after = "\\\\![x](https://evil.example/a.png)";
        assert!(images_in(after).len() == 1);
        let safe = safe_markdown(after, "github.com");
        assert!(images_in(&safe).is_empty(), "{safe}");
    }

    #[test]
    fn images_from_elsewhere_become_links() {
        assert_eq!(
            safe_markdown("a ![pixel](https://evil.example/x.png) b", "github.com"),
            "a [pixel](https://evil.example/x.png) b"
        );
        // Only the host counts, not what the address merely contains.
        assert_eq!(
            safe_markdown("![x](https://github.com.evil.example/x.png)", "github.com"),
            "[x](https://github.com.evil.example/x.png)"
        );
        assert_eq!(
            safe_markdown("![x](http://github.com/x.png)", "github.com"),
            "[x](http://github.com/x.png)"
        );
    }

    /// Where the viewer would fetch images from, as its own parser sees the text.
    fn image_sources(markdown: &str) -> Vec<String> {
        pulldown_cmark::Parser::new_ext(markdown, markdown_options())
            .filter_map(|event| match event {
                pulldown_cmark::Event::Start(pulldown_cmark::Tag::Image { dest_url, .. }) => {
                    Some(dest_url.to_string())
                }
                _ => None,
            })
            .collect()
    }

    #[test]
    fn no_way_of_writing_an_image_gets_one_fetched_from_elsewhere() {
        let evil = "https://evil.example/p.png";
        for body in [
            format!("![x][a]\n\n[a]: {evil}"),
            format!("![a][]\n\n[a]: {evil}"),
            format!("![a]\n\n[a]: {evil}"),
            format!("![a\\]b]({evil})"),
            format!("!![x]({evil})"),
            format!("!!!![x]({evil})"),
            format!("![![x]({evil})](https://github.com/a.png)"),
            format!("![![x](https://github.com/a.png)]({evil})"),
            format!("![x](<{evil}>)"),
            format!("![x]({evil} \"title\")"),
            format!("> ![x]({evil})"),
            format!("- ![x]({evil})"),
        ] {
            let shown = safe_markdown(&body, "github.com");
            assert!(
                image_sources(&shown)
                    .iter()
                    .all(|url| !url.contains("evil")),
                "{body:?} became {shown:?}"
            );
            assert!(shown.contains("evil.example"), "the link went: {shown:?}");
        }
    }

    #[test]
    fn a_table_outside_the_parsers_view_gets_no_image_through() {
        for body in [
            "~~~\n| h |\n|---|\n| ![x](https://evil.example/p.png) |\n~~~",
            "    | h |\n    |---|\n    | ![x](https://evil.example/p.png) |",
        ] {
            for block in safe_blocks(body, "github.com") {
                match block {
                    Block::Markdown(text) => {
                        assert!(image_sources(&text).is_empty(), "{body:?}: {text:?}");
                    }
                    Block::ImageTable { header, rows } => {
                        for cell in header.iter().chain(rows.iter().flatten()) {
                            assert!(image_sources(cell).is_empty(), "{body:?}: {cell:?}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn a_host_that_only_ends_right_is_no_trusted_host() {
        for url in [
            r"https://evil.example\.githubusercontent.com/p.png",
            "https://evil.example%2f.githubusercontent.com/p.png",
            "https://evil.example\t.githubusercontent.com/p.png",
            "https://user@evil.example/.githubusercontent.com",
        ] {
            assert!(!is_trusted_image(url, "github.com"), "{url}");
        }
        assert!(is_trusted_image(
            "https://user-images.githubusercontent.com/p.png",
            "github.com"
        ));
    }

    #[test]
    fn what_looks_like_an_image_in_code_stays_code() {
        let block = "```\n![x](https://evil.example/p.png)\n```";
        assert_eq!(safe_markdown(block, "github.com"), block);
        let inline = "Write `![x](https://evil.example/p.png)` for it";
        assert_eq!(safe_markdown(inline, "github.com"), inline);
    }

    #[test]
    fn an_image_link_without_text_still_has_something_to_click() {
        assert_eq!(
            safe_markdown("![](https://evil.example/x.png)", "github.com"),
            "[image](https://evil.example/x.png)"
        );
    }

    #[test]
    fn plain_text_with_exclamation_marks_is_left_alone() {
        let body = "Wow![not an image] and [link](https://a.example) ![unfinished](";
        assert_eq!(safe_markdown(body, "github.com"), body);
    }

    #[test]
    fn enterprise_hosts_trust_themselves() {
        let body = "![a](https://git.corp.example/x.png)";
        assert_eq!(safe_markdown(body, "git.corp.example"), body);
    }
}
