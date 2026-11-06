//! The name of the branch made for an issue: its number, then its title in a few words
//! (`42-fix-login-crash`); and why a name typed for it isn't one git takes.

/// How many characters of the title go in the name: enough to tell branches apart in a list,
/// short enough for a pill and a folder name.
const TITLE_CHARS: usize = 48;
/// And how many bytes: 48 characters of Devanagari or Chinese are three times that in UTF-8,
/// and the name is a folder's name too (most file systems stop at 255 bytes).
const TITLE_BYTES: usize = 120;

/// The zero-width non-joiner and joiner: inside a word in Persian, the Indic scripts and emoji
/// sequences (`می‌خواهم` is one word), never a gap.
const JOINERS: [char; 2] = ['\u{200C}', '\u{200D}'];

/// `42-fix-login-crash` for issue 42, "Fix login crash!". Letters and digits of any script stay
/// (lowercase), with the marks that go on them; everything else, spaces and punctuation and
/// emoji, becomes one `-` between words. A title with no words left gives `issue-42`. What
/// comes out is always a branch name git takes as it is: letters, digits, their marks and
/// single dashes, starting with the number.
pub(crate) fn branch_name(number: u64, title: &str) -> String {
    let words = slug(title);
    if words.is_empty() {
        format!("issue-{number}")
    } else {
        format!("{number}-{words}")
    }
}

/// The title's words, lowercase, joined by single dashes, cut after [`TITLE_CHARS`] characters
/// (or [`TITLE_BYTES`] bytes) at a word's end when one is near (else within the word, on a
/// character's boundary).
fn slug(title: &str) -> String {
    let mut out = String::new();
    let mut gap = false;
    // A joiner waits for the letter after it: at a word's end it would be a gap no one sees.
    let mut joiner = None;
    for c in title.chars() {
        if c.is_alphanumeric() {
            if gap && !out.is_empty() {
                out.push('-');
            } else if let Some(j) = joiner {
                out.push(j);
            }
            gap = false;
            joiner = None;
            // A letter's lowercase can be more than one character (`İ` is `i` and a combining
            // dot); only letters and digits of it are kept.
            out.extend(c.to_lowercase().filter(|l| l.is_alphanumeric()));
        } else if !gap && !out.is_empty() && JOINERS.contains(&c) {
            joiner = Some(c);
        } else if !gap && !out.is_empty() && joiner.is_none() && is_mark(c) {
            // The `̈` of a `ü` written as `u` and `̈` (as macOS names files), a virama.
            out.push(c);
        } else {
            gap = true;
            joiner = None;
        }
    }
    shorten(&out)
}

/// Whether `c` is a combining mark that isn't a letter itself (Rust's `is_alphanumeric` already
/// takes most vowel signs). No table of Unicode's categories is at hand, so these are the
/// blocks of them that titles have: the combining diacritics (decomposed Latin, Greek and
/// Cyrillic), Hebrew and Arabic points, the signs and viramas of the Indic scripts, Thai, Lao,
/// Myanmar and Khmer tone marks and viramas, the kana voicing marks and variation selectors. A
/// mark outside them is a gap, as before: the name splits there, and git still takes it.
fn is_mark(c: char) -> bool {
    let u = u32::from(c);
    // The Indic blocks share one layout, 0x80 wide from Devanagari to Malayalam: signs at the
    // start (candrabindu, anusvara, visarga), nukta to virama and length marks in the middle,
    // vocalic vowel signs after.
    let indic = (0x0900..=0x0D7F).contains(&u)
        && matches!(
            (u - 0x0900) % 0x80,
            0x00..=0x03 | 0x3A..=0x4F | 0x51..=0x57 | 0x62..=0x63
        );
    indic
        || matches!(
            u,
            0x0300..=0x036F
                | 0x0483..=0x0489
                | 0x0591..=0x05BD
                | 0x05BF
                | 0x05C1..=0x05C2
                | 0x05C4..=0x05C5
                | 0x05C7
                | 0x0610..=0x061A
                | 0x064B..=0x065F
                | 0x0670
                | 0x06D6..=0x06DC
                | 0x06DF..=0x06E4
                | 0x06E7..=0x06E8
                | 0x06EA..=0x06ED
                | 0x0DCA
                | 0x0DCF..=0x0DDF
                | 0x0E31
                | 0x0E34..=0x0E3A
                | 0x0E47..=0x0E4E
                | 0x0EB1
                | 0x0EB4..=0x0EBC
                | 0x0EC8..=0x0ECD
                | 0x102B..=0x103E
                | 0x17B4..=0x17D3
                | 0x17DD
                | 0x1AB0..=0x1AFF
                | 0x1DC0..=0x1DFF
                | 0x20D0..=0x20FF
                | 0x3099..=0x309A
                | 0xFE00..=0xFE0F
                | 0xFE20..=0xFE2F
        )
}

/// `slug` cut to [`TITLE_CHARS`] characters and [`TITLE_BYTES`] bytes, with no dash (or
/// joiner) left at its end.
fn shorten(slug: &str) -> String {
    if slug.chars().count() <= TITLE_CHARS && slug.len() <= TITLE_BYTES {
        return slug.to_owned();
    }
    let end = slug
        .char_indices()
        .map(|(i, c)| i + c.len_utf8())
        .take(TITLE_CHARS)
        .take_while(|end| *end <= TITLE_BYTES)
        .last()
        .unwrap_or(0);
    let cut = &slug[..end];
    // At the last word's end, unless that leaves too little of the title.
    let at_word = match cut.rfind('-') {
        Some(dash) if cut[..dash].chars().count() >= TITLE_CHARS / 2 => &cut[..dash],
        _ => cut,
    };
    at_word
        .trim_end_matches(|c| c == '-' || JOINERS.contains(&c))
        .to_owned()
}

/// Why git wouldn't take `name` as a branch's name (`git check-ref-format --branch`, and never
/// as an option), in a few words for the picker; `None` when it would.
pub(crate) fn name_problem(name: &str) -> Option<String> {
    let bad = name
        .chars()
        .find(|c| c.is_control() || matches!(c, ' ' | '~' | '^' | ':' | '?' | '*' | '[' | '\\'));
    let parts = || name.split('/');
    let why = if name.is_empty() {
        "The branch needs a name".to_owned()
    } else if name.starts_with('-') {
        "A branch's name can't start with -".to_owned()
    } else if name == "HEAD" || name == "@" {
        format!("{name} is a name git keeps for itself")
    } else if let Some(c) = bad {
        if c.is_control() {
            "A branch's name can't have control characters in it".to_owned()
        } else {
            format!("A branch's name can't have {c} in it")
        }
    } else if let Some(two) = ["..", "@{"].into_iter().find(|s| name.contains(s)) {
        format!("A branch's name can't have {two} in it")
    } else if parts().any(str::is_empty) {
        "A branch's name can't start or end with /, nor have // in it".to_owned()
    } else if parts().any(|part| part.starts_with('.')) {
        "No part of a branch's name (between /) can start with .".to_owned()
    } else if parts().any(|part| part.ends_with(".lock")) {
        "No part of a branch's name (between /) can end with .lock".to_owned()
    } else if name.ends_with('.') {
        "A branch's name can't end with .".to_owned()
    } else {
        return None;
    };
    Some(why)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Whether git takes `name` as a branch name as it is.
    fn git_takes(name: &str) -> bool {
        crate::git::run(None, &["check-ref-format", "--branch", name])
            .is_ok_and(|out| out.trim() == name)
    }

    #[test]
    fn number_then_the_titles_words() {
        assert_eq!(branch_name(42, "Fix login crash"), "42-fix-login-crash");
        assert_eq!(
            branch_name(7, "  [UI]  Crash: when   saving…  "),
            "7-ui-crash-when-saving"
        );
        assert_eq!(branch_name(3, "v2.0 / API_rework"), "3-v2-0-api-rework");
    }

    #[test]
    fn letters_of_any_script_stay() {
        assert_eq!(
            branch_name(5, "Füße — ünïcode 日本語"),
            "5-füße-ünïcode-日本語"
        );
        assert_eq!(branch_name(6, "İstanbul"), "6-istanbul");
    }

    #[test]
    fn marks_and_joiners_stay_in_their_word() {
        // `ü` as `u` and a combining diaeresis.
        assert_eq!(branch_name(1, "Fu\u{308}ße crash"), "1-fu\u{308}ße-crash");
        // A virama between consonants (क्र), a vowel sign that isn't a letter for Rust.
        assert_eq!(branch_name(2, "लॉगिन क्रैश"), "2-लॉगिन-क्रैश");
        // Persian's zero-width non-joiner, inside the word.
        assert_eq!(branch_name(3, "می‌خواهم"), "3-می‌خواهم");
        // A joiner or a mark with no letter before it, or at a word's end, is a gap.
        assert_eq!(branch_name(4, "a\u{200C} \u{308}b"), "4-a-b");
        assert_eq!(branch_name(5, "\u{200D}x"), "5-x");
    }

    #[test]
    fn emoji_and_punctuation_are_gaps() {
        assert_eq!(branch_name(8, "🔥 Hot fix 🚀🚀"), "8-hot-fix");
        assert_eq!(branch_name(9, "a👍b"), "9-a-b");
    }

    #[test]
    fn a_title_without_words_gives_the_number_alone() {
        assert_eq!(branch_name(10, ""), "issue-10");
        assert_eq!(branch_name(11, "   "), "issue-11");
        assert_eq!(branch_name(12, "?!… — ~^:*[]\\ .. @{ 🎉"), "issue-12");
    }

    #[test]
    fn a_long_title_is_cut_at_a_word() {
        let long = "Make the settings page remember which section was open last time and \
                    scroll back to it";
        let name = branch_name(99, long);
        assert_eq!(name, "99-make-the-settings-page-remember-which-section");
        assert!(name.chars().count() <= TITLE_CHARS + 3);
    }

    #[test]
    fn a_long_word_is_cut_on_a_characters_boundary() {
        // Three bytes a character: the bytes run out first.
        let name = branch_name(1, &"日本".repeat(100));
        assert_eq!(name.len(), 2 + TITLE_BYTES);
        assert!(name.starts_with("1-日本日本"));
        let mixed = branch_name(2, &format!("{}é{}", "a".repeat(47), "b".repeat(20)));
        assert_eq!(mixed, format!("2-{}é", "a".repeat(47)));
        let devanagari = branch_name(3, &"क्रैश ".repeat(40));
        assert!(devanagari.len() <= 2 + TITLE_BYTES, "{devanagari}");
        assert!(devanagari.ends_with('श'), "{devanagari}");
    }

    #[test]
    fn every_name_is_one_git_takes() {
        let titles = [
            "Fix login crash",
            "Füße — ünïcode 日本語",
            "Fu\u{308}ße crash",
            "लॉगिन क्रैश",
            "می‌خواهم",
            "🔥🔥🔥",
            "",
            "...lock",
            "name.lock",
            "-D --force",
            "a..b @{u} ~1 ^2 :x ?y *z [w] \\v",
            "trailing dot.",
            "/leading/slash/",
            "tab\tand\nnewline",
            "İıß ﬁ Ǆ",
            &"very long title ".repeat(40),
            &"ü".repeat(500),
            &"می‌".repeat(100),
        ];
        for title in titles {
            let name = branch_name(1234, title);
            assert!(git_takes(&name), "{title:?} → {name:?}");
            assert!(name_problem(&name).is_none(), "{name:?}");
            assert!(!name.ends_with('-') && !name.contains("--"), "{name:?}");
        }
    }

    #[test]
    fn a_name_git_refuses_says_why() {
        let names = [
            "42-fix",
            "feature/x",
            "x.lock",
            "feature/x.lock/y",
            "a/.b",
            ".a",
            "a/",
            "/a",
            "a//b",
            "a..b",
            "a@{b",
            "a b",
            "a~1",
            "a^",
            "a:b",
            "a?",
            "a*",
            "a[b",
            "a\\b",
            "a.",
            "ünï/日本",
            "a@b",
            "tab\t",
        ];
        for name in names {
            assert_eq!(
                name_problem(name).is_none(),
                git_takes(name),
                "{name:?}: {:?}",
                name_problem(name)
            );
        }
        assert!(name_problem("").is_some());
        assert!(name_problem("-D").is_some());
        // git would take it (`refs/heads/@`), but `@` is HEAD wherever git reads a revision.
        assert!(name_problem("@").is_some());
        assert_eq!(
            name_problem("HEAD").as_deref(),
            Some("HEAD is a name git keeps for itself")
        );
        assert_eq!(
            name_problem("x/y.lock").as_deref(),
            Some("No part of a branch's name (between /) can end with .lock")
        );
    }
}
