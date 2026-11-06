//! What a single line of code is, as far as the checks are concerned: an assertion, a test, a
//! skip, an assertion that can't fail. Matched on the text alone, the same way every time.

/// The line without its indentation, or `None` when it is a comment (a commented-out
/// assertion is no assertion).
pub fn code(text: &str) -> Option<&str> {
    let t = text.trim();
    let comment = ["//", "#", "/*", "*", "--", ";", "<!--"]
        .iter()
        .any(|c| t.starts_with(c))
        // Rust and C# attributes start with `#[` / `[`, Python decorators with `@`: code.
        && !t.starts_with("#[")
        && !t.starts_with("#!");
    (!t.is_empty() && !comment).then_some(t)
}

/// Whether `word` is in `text` with no letter, digit or `_` right before it.
fn starts_word(text: &str, word: &str) -> bool {
    text.match_indices(word).any(|(at, _)| {
        text[..at]
            .chars()
            .next_back()
            .is_none_or(|c| !(c.is_alphanumeric() || c == '_'))
    })
}

/// Whether `call` starts a word of `text` and its first argument is a string.
fn starts_named_call(text: &str, call: &str) -> bool {
    text.match_indices(call).any(|(at, _)| {
        let before_ok = text[..at]
            .chars()
            .next_back()
            .is_none_or(|c| !(c.is_alphanumeric() || c == '_'));
        let arg = text[at + call.len()..].trim_start();
        before_ok && arg.starts_with(['\'', '"', '`'])
    })
}

/// An assertion, in the usual test frameworks.
pub fn is_assertion(text: &str) -> bool {
    let Some(t) = code(text) else {
        return false;
    };
    let lower = t.to_lowercase();
    starts_word(&lower, "assert")
        || lower.contains("xctassert")
        || starts_word(&lower, "expect(")
        || starts_word(&lower, "expect!(")
        || lower.contains(".should")
        || starts_word(&lower, "require.")
        || starts_word(t, "t.Error")
        || starts_word(t, "t.Fatal")
        || starts_word(&lower, "verify(")
}

/// The start of a test: `#[test]`, `def test_…`, `it(…`, `func TestX`, `@Test`, `[Fact]`…
pub fn is_test_start(text: &str) -> bool {
    let Some(t) = code(text) else {
        return false;
    };
    let lower = t.to_lowercase();
    lower.starts_with("#[test]")
        || lower.starts_with("#[tokio::test")
        || lower.starts_with("#[rstest")
        || lower.starts_with("def test")
        || lower.starts_with("async def test")
        || lower.starts_with("@test")
        || lower.starts_with("[fact")
        || lower.starts_with("[theory")
        || lower.starts_with("[test")
        || t.starts_with("func Test")
        || ["it(", "test(", "it.each", "test.each", "it \"", "it '"]
            .iter()
            .any(|s| lower.starts_with(s))
}

/// Why `text` skips or disables tests, if it does: the words that do it.
pub fn skip_marker(text: &str, test_file: bool) -> Option<&'static str> {
    let t = code(text)?;
    let lower = t.to_lowercase();
    const STARTS: [&str; 8] = [
        "#[ignore",
        "#[cfg(any())]",
        "@pytest.mark.skip",
        "@pytest.mark.xfail",
        "@unittest.skip",
        "@disabled",
        "@ignore",
        "[ignore",
    ];
    if let Some(found) = STARTS.iter().find(|m| lower.starts_with(*m)) {
        return Some(found);
    }
    const WORDS: [&str; 4] = ["pytest.skip(", "self.skiptest(", "t.skip(", "t.skipnow("];
    if let Some(found) = WORDS.iter().find(|w| starts_word(&lower, w)) {
        return Some(found);
    }
    // Jasmine, Jest, Mocha, Vitest: a test's name comes first, in quotes (`it.skip(2)` is an
    // iterator, `fit(model, data)` a model being fitted).
    const NAMED: [&str; 13] = [
        "it.skip(",
        "test.skip(",
        "describe.skip(",
        "it.todo(",
        "test.todo(",
        "it.only(",
        "test.only(",
        "describe.only(",
        "xit(",
        "xtest(",
        "xdescribe(",
        "fit(",
        "fdescribe(",
    ];
    if let Some(found) = NAMED.iter().find(|w| starts_named_call(&lower, w)) {
        return Some(found);
    }
    // C#: `[Fact(Skip = "…")]`.
    if (lower.starts_with("[fact") || lower.starts_with("[theory")) && lower.contains("skip") {
        return Some("Skip =");
    }
    // RSpec / minitest words are too common elsewhere: only in test files.
    if test_file {
        for word in ["skip ", "skip(", "pending ", "pending(", "xit "] {
            if lower.starts_with(word) {
                return Some(word.trim_end_matches(['(', ' ']));
            }
        }
    }
    None
}

/// An assertion that can never fail: `assert True`, `assert!(true)`, `expect(true).toBe(true)`.
pub fn is_trivial_assertion(text: &str) -> bool {
    let Some(t) = code(text) else {
        return false;
    };
    let squeezed: String = t
        .to_lowercase()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    let squeezed = squeezed.trim_end_matches(';');
    const ALWAYS: [&str; 14] = [
        "asserttrue",
        "assert1",
        "assert!(true)",
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
        "xctasserttrue(true)",
    ];
    ALWAYS.contains(&squeezed) || squeezed == "assert!(1==1)" || squeezed == "assert1==1"
}
