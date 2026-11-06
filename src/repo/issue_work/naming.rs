//! The name of the branch made for an issue: its number, then its title in a few words
//! (`42-fix-login-crash`).

/// How many characters of the title go in the name: enough to tell branches apart in a list,
/// short enough for a pill and a folder name.
const TITLE_CHARS: usize = 48;

/// `42-fix-login-crash` for issue 42, "Fix login crash!". Letters and digits of any script stay
/// (lowercase); everything else, spaces and punctuation and emoji, becomes one `-` between
/// words. A title with no words left gives `issue-42`. What comes out is always a branch name
/// git takes as it is: letters, digits and single dashes, starting with the number.
pub(crate) fn branch_name(number: u64, title: &str) -> String {
    let words = slug(title);
    if words.is_empty() {
        format!("issue-{number}")
    } else {
        format!("{number}-{words}")
    }
}

/// The title's words, lowercase, joined by single dashes, cut after [`TITLE_CHARS`] characters
/// at a word's end when one is near (else within the word, on a character's boundary).
fn slug(title: &str) -> String {
    let mut out = String::new();
    let mut gap = false;
    for c in title.chars() {
        if c.is_alphanumeric() {
            if gap && !out.is_empty() {
                out.push('-');
            }
            gap = false;
            // A letter's lowercase can be more than one character (`İ` is `i` and a combining
            // dot); only letters and digits of it are kept.
            out.extend(c.to_lowercase().filter(|l| l.is_alphanumeric()));
        } else {
            gap = true;
        }
    }
    shorten(&out)
}

/// `slug` cut to [`TITLE_CHARS`] characters, with no dash left at its end.
fn shorten(slug: &str) -> String {
    if slug.chars().count() <= TITLE_CHARS {
        return slug.to_owned();
    }
    let cut: String = slug.chars().take(TITLE_CHARS).collect();
    // At the last word's end, unless that leaves too little of the title.
    let at_word = match cut.rfind('-') {
        Some(dash) if cut[..dash].chars().count() >= TITLE_CHARS / 2 => &cut[..dash],
        _ => cut.as_str(),
    };
    at_word.trim_end_matches('-').to_owned()
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
        let name = branch_name(1, &"日本".repeat(100));
        assert_eq!(name.chars().count(), 2 + TITLE_CHARS);
        assert!(name.starts_with("1-日本日本"));
        let mixed = branch_name(2, &format!("{}é{}", "a".repeat(47), "b".repeat(20)));
        assert_eq!(mixed, format!("2-{}é", "a".repeat(47)));
    }

    #[test]
    fn every_name_is_one_git_takes() {
        let titles = [
            "Fix login crash",
            "Füße — ünïcode 日本語",
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
        ];
        for title in titles {
            let name = branch_name(1234, title);
            assert!(git_takes(&name), "{title:?} → {name:?}");
            assert!(!name.ends_with('-') && !name.contains("--"), "{name:?}");
        }
    }
}
