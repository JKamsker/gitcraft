//! Keeping tool results to a size an agent can read: long text in pages, long lists cut with
//! what was left out said.

use serde_json::{Value, json};

/// A page of `text` from byte `offset`, at most `max` bytes, cut at a character (and, when it
/// can, a line) boundary. `next_offset` says where the next page starts, when there is one.
pub fn text_page(text: &str, offset: usize, max: usize) -> Value {
    let total = text.len();
    let start = floor_char(text, offset.min(total));
    let mut end = floor_char(text, start.saturating_add(max).min(total));
    if end < total {
        // Whole lines when a line ends in the page.
        if let Some(nl) = text[start..end].rfind('\n') {
            end = start + nl + 1;
        }
    }
    // Always some progress, even on a line longer than the page.
    if end == start && start < total {
        end = ceil_char(text, start + 1);
    }
    let mut page = json!({
        "text": &text[start..end],
        "total_bytes": total,
    });
    if end < total {
        page["next_offset"] = json!(end);
        page["truncated"] = json!(format!(
            "{} more bytes: call again with offset {end}",
            total - end
        ));
    }
    page
}

fn floor_char(text: &str, mut i: usize) -> usize {
    while i > 0 && !text.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn ceil_char(text: &str, mut i: usize) -> usize {
    while i < text.len() && !text.is_char_boundary(i) {
        i += 1;
    }
    i
}

/// At most `max` of `items`, and how many more there were when cut.
pub fn capped<T>(items: Vec<T>, max: usize) -> (Vec<T>, usize) {
    let more = items.len().saturating_sub(max);
    let mut items = items;
    items.truncate(max);
    (items, more)
}

/// A remote's URL without a password or token in it (`https://user:token@host/x`, or a token
/// as the user name of `https://ghp_x@host/x`): nothing for an agent to read. An SSH URL keeps
/// its user name (`ssh://git@host/x`), which is no secret.
pub fn without_credentials(url: &str) -> String {
    let Some((scheme, rest)) = url.split_once("://") else {
        return url.to_owned();
    };
    let (host_part, path) = match rest.find('/') {
        Some(i) => rest.split_at(i),
        None => (rest, ""),
    };
    let Some((user, host)) = host_part.rsplit_once('@') else {
        return url.to_owned();
    };
    // `helper::https://…` goes through a remote helper, over https all the same.
    let last = scheme.rsplit("::").next().unwrap_or(scheme);
    let web = matches!(last.to_lowercase().as_str(), "http" | "https");
    match user.split_once(':') {
        Some((name, _)) if !web => format!("{scheme}://{name}@{host}{path}"),
        None if !web => url.to_owned(),
        _ => format!("{scheme}://{host}{path}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_text_comes_in_pages_of_whole_lines() {
        let text = "one\ntwo\nthree\n";
        let page = text_page(text, 0, 9);
        assert_eq!(page["text"], "one\ntwo\n");
        assert_eq!(page["next_offset"], 8);
        let rest = text_page(text, 8, 100);
        assert_eq!(rest["text"], "three\n");
        assert!(rest.get("next_offset").is_none());
        assert_eq!(rest["total_bytes"], 14);
    }

    #[test]
    fn a_page_never_splits_a_character_nor_stalls() {
        let text = "ééééé";
        let page = text_page(text, 1, 3);
        // From the start of the character offset 1 is inside, up to a whole one.
        assert_eq!(page["text"], "é");
        let stuck = text_page("x".repeat(10).as_str(), 0, 0);
        assert_eq!(stuck["text"], "x");
        let past = text_page("abc", 99, 10);
        assert_eq!(past["text"], "");
    }

    #[test]
    fn lists_say_what_was_left_out() {
        assert_eq!(capped(vec![1, 2, 3], 2), (vec![1, 2], 1));
        assert_eq!(capped(vec![1], 2), (vec![1], 0));
    }

    #[test]
    fn credentials_never_leave_with_a_url() {
        assert_eq!(
            without_credentials("https://me:ghp_secret@github.com/a/b.git"),
            "https://github.com/a/b.git"
        );
        assert_eq!(
            without_credentials("https://github.com/a/b@c.git"),
            "https://github.com/a/b@c.git"
        );
        assert_eq!(
            without_credentials("git@github.com:a/b.git"),
            "git@github.com:a/b.git"
        );
        assert_eq!(without_credentials("ssh://git@host/x"), "ssh://git@host/x");
        assert_eq!(
            without_credentials("ssh://git:pw@host/x"),
            "ssh://git@host/x"
        );
        assert_eq!(
            without_credentials("https://ghp_x@host/x"),
            "https://host/x"
        );
    }
}
