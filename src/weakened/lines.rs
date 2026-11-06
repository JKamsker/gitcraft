//! What a single line of code is, as far as the checks are concerned: an assertion, a test, a
//! skip, an assertion that can't fail. Matched on the text alone, the same way every time.

/// How a file's lines are read.
#[derive(Clone, Copy, Debug, Default)]
pub struct Lang {
    /// A file of tests (by its path).
    pub test_file: bool,
    /// `--` starts a comment (SQL, Lua, Haskell).
    pub dash_comments: bool,
}

/// The line without its indentation, or `None` when it is a comment (a commented-out
/// assertion is no assertion).
pub fn code(text: &str, lang: Lang) -> Option<&str> {
    let t = text.trim();
    let comment = t.starts_with("//")
        || t.starts_with("/*")
        || t.starts_with("<!--")
        // `#[…]` (Rust attributes) and `#!` are code; `#` alone a comment in shell, Python, YAML…
        || (t.starts_with('#') && !t.starts_with("#[") && !t.starts_with("#!"))
        // A block comment's next line: ` * text`, not `*p = 1` or `*a, b = t`.
        || t == "*"
        || t.starts_with("* ")
        || t.starts_with("*/")
        || (lang.dash_comments && t.starts_with("--"));
    (!t.is_empty() && !comment).then_some(t)
}

/// Whether byte offset `at` of `text` is inside a string literal (`"…"`, `'…'` or `` `…` ``),
/// as far as counting the quotes before it tells.
pub fn in_string(text: &str, at: usize) -> bool {
    let mut open: Option<char> = None;
    let mut escaped = false;
    for c in text[..at.min(text.len())].chars() {
        if escaped {
            escaped = false;
            continue;
        }
        match (open, c) {
            (_, '\\') => escaped = true,
            (None, '"' | '\'' | '`') => open = Some(c),
            (Some(q), c) if c == q => open = None,
            _ => {}
        }
    }
    open.is_some()
}

/// Places of `word` in `text` that start a word (no letter, digit, `_` or `.` right before)
/// and are not inside a string.
fn word_at<'a>(text: &'a str, word: &'a str) -> impl Iterator<Item = usize> + 'a {
    starts_at(text, word, false)
}

/// [`word_at`], a `.` before the word allowed when `after_dot` (a method: `self.assertTrue`).
fn starts_at<'a>(
    text: &'a str,
    word: &'a str,
    after_dot: bool,
) -> impl Iterator<Item = usize> + 'a {
    text.match_indices(word)
        .map(|(at, _)| at)
        .filter(move |&at| {
            let before = text[..at].chars().next_back();
            let free = before
                .is_none_or(|c| !(c.is_alphanumeric() || c == '_' || (c == '.' && !after_dot)));
            free && !in_string(text, at)
        })
}

fn has_word(text: &str, word: &str) -> bool {
    word_at(text, word).next().is_some()
}

/// [`has_word`] for a method too (`self.assertEqual`, `Assert.Equal`).
fn has_method(text: &str, word: &str) -> bool {
    starts_at(text, word, true).next().is_some()
}

/// Whether `text` at `at` (after a matched word) ends the word: no letter, digit or `_` next.
fn ends_word(text: &str, at: usize) -> bool {
    text[at..]
        .chars()
        .next()
        .is_none_or(|c| !(c.is_alphanumeric() || c == '_'))
}

/// `text` starts with `word` as a whole word (`@Ignore` but not `@IgnoreExtraProperties`).
fn starts_with_word(text: &str, word: &str) -> bool {
    text.starts_with(word) && ends_word(text, word.len())
}

/// An assertion, in the usual test frameworks. Outside test files only `assert…` and
/// `debug_assert…` count (`.expect()` is no assertion in Rust, `require.resolve` none in JS).
pub fn is_assertion(text: &str, lang: Lang) -> bool {
    let Some(t) = code(text, lang) else {
        return false;
    };
    let lower = t.to_lowercase();
    let anywhere = has_method(&lower, "assert")
        || has_word(&lower, "debug_assert")
        || has_word(&lower, "prop_assert")
        || has_word(&lower, "xctassert");
    anywhere
        || (lang.test_file
            && (has_word(&lower, "expect(")
                || lower.contains(".should")
                || has_word(&lower, "require.")
                || has_word(t, "t.Error")
                || has_word(t, "t.Fatal")))
}

/// The start of a test: `#[test]`, `def test_…`, `it("…"`, `func TestX`, `@Test`, `[Fact]`…
pub fn is_test_start(text: &str, lang: Lang) -> bool {
    let Some(t) = code(text, lang) else {
        return false;
    };
    let lower = t.to_lowercase();
    let attribute = ["#[test]", "#[tokio::test", "#[rstest", "#[test_case"]
        .iter()
        .any(|a| lower.starts_with(a));
    let annotation = [
        "@test",
        "[fact",
        "[theory",
        "[test",
        "[testmethod",
        "[testcase",
    ]
    .iter()
    .any(|a| starts_with_word(&lower, a));
    let function = lower.starts_with("def test")
        || lower.starts_with("async def test")
        || t.starts_with("func Test");
    let named = ["it(", "test(", "it.each", "test.each", "it ", "specify("]
        .iter()
        .any(|s| lower.starts_with(s) && named_after(&lower[s.len()..]));
    attribute || annotation || function || named
}

/// Whether what follows a call's name starts with a quoted name (`it("adds", …)`).
fn named_after(rest: &str) -> bool {
    let rest = rest.trim_start_matches(['(', ' ', '`']);
    rest.starts_with(['\'', '"']) || rest.starts_with("`")
}

/// Why `text` skips, disables or focuses tests, if it does: the words that do it, as written.
pub fn skip_marker(text: &str, lang: Lang) -> Option<String> {
    let t = code(text, lang)?;
    let lower = t.to_lowercase();
    let shown = |at: usize, len: usize| t.get(at..at + len).unwrap_or(t).to_owned();
    // Attributes and annotations, as whole words.
    const ATTRIBUTES: [&str; 10] = [
        "#[ignore",
        "#[cfg(any())]",
        "@pytest.mark.skip",
        "@pytest.mark.xfail",
        "@unittest.skip",
        "@unittest.expectedfailure",
        "@disabled",
        "@ignore",
        "[ignore",
        "[skip",
    ];
    if let Some(a) = ATTRIBUTES.iter().find(|a| starts_with_word(&lower, a)) {
        return Some(shown(0, a.len()));
    }
    if lower.starts_with("#[cfg_attr(") && lower.contains("ignore") {
        return Some(shown(0, lower.find(')').map_or(lower.len(), |e| e + 1)));
    }
    // C#: `[Fact(Skip = "…")]`.
    if (lower.starts_with("[fact") || lower.starts_with("[theory")) && lower.contains("skip") {
        return Some(shown(0, t.len()));
    }
    // Calls that skip from inside a test.
    for call in [
        "pytest.skip(",
        "self.skiptest(",
        "t.skip(",
        "t.skipnow(",
        "pytest.xfail(",
    ] {
        if let Some(at) = word_at(&lower, call).next() {
            return Some(shown(at, call.len() - 1));
        }
    }
    // Jasmine, Jest, Mocha, Vitest: a test's name comes first, in quotes (`it.skip(2)` is an
    // iterator, `model.fit("x")` a model being fitted, both no test).
    const NAMED: [&str; 11] = [
        "it.skip(",
        "test.skip(",
        "describe.skip(",
        "it.only(",
        "test.only(",
        "describe.only(",
        "xit(",
        "xtest(",
        "xdescribe(",
        "fit(",
        "fdescribe(",
    ];
    for call in NAMED {
        if let Some(at) = word_at(&lower, call).find(|&at| named_after(&lower[at + call.len()..])) {
            return Some(shown(at, call.len() - 1));
        }
    }
    // RSpec / minitest: `skip "why"`, `pending "why"`, `xit "name"`, only in test files.
    if lang.test_file {
        for word in ["skip", "pending", "xit"] {
            let rest = lower
                .strip_prefix(word)
                .filter(|_| ends_word(&lower, word.len()));
            if rest.is_some_and(|r| r.trim().is_empty() || named_after(r)) {
                return Some(shown(0, word.len()));
            }
        }
    }
    None
}

/// An assertion that can never fail: `assert True`, `assert!(true)`, `expect(true).toBe(true)`.
pub fn is_trivial_assertion(text: &str, lang: Lang) -> bool {
    let Some(t) = code(text, lang) else {
        return false;
    };
    let squeezed: String = t
        .to_lowercase()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    let squeezed = squeezed.trim_end_matches(';');
    const ALWAYS: [&str; 15] = [
        "asserttrue",
        "assert1",
        "assert1==1",
        "assert!(true)",
        "assert!(1==1)",
        "assert(true)",
        "assert_eq!(true,true)",
        "asserttrue(true)",
        "self.asserttrue(true)",
        "assert.true(true)",
        "assert.istrue(true)",
        "assert.ok(true)",
        "expect(true).tobe(true)",
        "expect(true).toequal(true)",
        "expect(true).tobetruthy()",
    ];
    // Python's `assert True, "message"`.
    let message = ["asserttrue,", "assert1,"]
        .iter()
        .any(|p| squeezed.starts_with(p));
    ALWAYS.contains(&squeezed) || message || squeezed == "xctasserttrue(true)"
}
