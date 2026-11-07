//! Syntax colouring for diffs (syntect with bat's syntaxes), chosen by file extension or name.
//! The default build uses Oniguruma; `--no-default-features` keeps the pure-Rust backend.

use std::sync::OnceLock;

use egui::Color32;
use syntect::highlighting::{HighlightIterator, HighlightState, Highlighter, Theme, ThemeSet};
use syntect::parsing::{ParseState, ScopeStack, ScopeStackOp, SyntaxReference, SyntaxSet};

/// Coloured pieces of one line.
pub type Spans = Vec<(Color32, String)>;

/// Bigger diffs stay uncoloured: highlighting is the slow part of opening them.
const MAX_LINES: usize = 8000;
/// Nor much text in few lines (a minified bundle): colouring takes about 2 s per MB.
const MAX_BYTES: usize = 512 * 1024;

fn syntaxes() -> &'static SyntaxSet {
    static SET: OnceLock<SyntaxSet> = OnceLock::new();
    SET.get_or_init(two_face::syntax::extra_newlines)
}

fn themes() -> &'static ThemeSet {
    static SET: OnceLock<ThemeSet> = OnceLock::new();
    SET.get_or_init(ThemeSet::load_defaults)
}

/// Colours `lines` (one file's text, in order) for `path`; `None` for plain text, unknown
/// types or very long inputs.
pub fn highlight<'a>(
    path: &str,
    lines: impl ExactSizeIterator<Item = &'a str>,
    dark: bool,
) -> Option<Vec<Spans>> {
    if lines.len() > MAX_LINES {
        return None;
    }
    let lines: Vec<&str> = lines.collect();
    if lines.iter().map(|l| l.len()).sum::<usize>() > MAX_BYTES {
        return None;
    }
    let ss = syntaxes();
    let file = std::path::Path::new(path);
    let name = file.file_name()?.to_string_lossy();
    // By the whole name for files like `Dockerfile` and `CMakeLists.txt` (whose `.txt` alone is
    // plain text), or else by extension.
    let syntax = ss.find_syntax_by_extension(&name).or_else(|| {
        file.extension()
            .and_then(|e| ss.find_syntax_by_extension(&e.to_string_lossy()))
    })?;
    if syntax.name == "Plain Text" {
        return None;
    }
    // VS Code's theme when it suits the background: a dark theme on a light gitcraft reads badly.
    let editor = crate::vscode_theme::enabled()
        .then(crate::vscode_theme::current)
        .flatten()
        .filter(|e| e.dark == dark);
    let theme = match &editor {
        Some(e) => &e.theme,
        None => themes().themes.get(if dark {
            "base16-ocean.dark"
        } else {
            "InspiredGitHub"
        })?,
    };
    colour_lines(lines, syntax, theme, ss)
}

struct CachedContext<'a> {
    state: ParseState,
    lines: std::collections::HashMap<&'a str, Vec<(usize, ScopeStackOp)>>,
}

fn colour_lines(
    lines: Vec<&str>,
    syntax: &SyntaxReference,
    theme: &Theme,
    ss: &SyntaxSet,
) -> Option<Vec<Spans>> {
    let highlighter = Highlighter::new(theme);
    let mut state = HighlightState::new(&highlighter, ScopeStack::new());
    let mut parser = ParseState::new(syntax);
    // Generated files repeat lines such as `[[package]]` and registry URLs thousands of times.
    // Reuse operations only when the complete parser state is identical before and after a
    // line. Apply them to the current highlight state, preserving multiline scopes and themes.
    // At most four contexts and 128 short lines in total, dropped when this file finishes.
    let mut contexts: Vec<CachedContext<'_>> = Vec::new();
    let mut cached_lines = 0;
    let can_cache = lines.len() >= 256 && lines.first().is_some_and(|l| l.len() <= 256);
    let mut out = Vec::with_capacity(lines.len());
    let mut with_nl = String::new();
    for line in lines {
        with_nl.clear();
        with_nl.push_str(line);
        with_nl.push('\n');
        let context = contexts.iter().position(|c| c.state == parser);
        let parsed;
        let ops = if let Some(ops) = context.and_then(|i| contexts[i].lines.get(line)) {
            ops
        } else {
            parsed = parser.parse_line(&with_nl, ss).ok()?;
            if let Some(i) = context
                && contexts[i].state == parser
                && line.len() <= 256
                && cached_lines < 128
            {
                contexts[i].lines.insert(line, parsed.clone());
                cached_lines += 1;
            } else if can_cache
                && contexts.len() < 4
                && line.len() <= 256
                && !contexts.iter().any(|c| c.state == parser)
            {
                contexts.push(CachedContext {
                    state: parser.clone(),
                    lines: Default::default(),
                });
            }
            &parsed
        };
        let ranges = HighlightIterator::new(&mut state, ops, &with_nl, &highlighter);
        let mut spans: Spans = Vec::new();
        for (style, text) in ranges {
            let text = text.trim_end_matches('\n');
            if text.is_empty() {
                continue;
            }
            let c = style.foreground;
            let colour = Color32::from_rgb(c.r, c.g, c.b);
            if let Some((last_colour, last_text)) = spans.last_mut()
                && *last_colour == colour
            {
                last_text.push_str(text);
            } else {
                spans.push((colour, text.to_owned()));
            }
        }
        out.push(spans);
    }
    Some(out)
}

/// Characters of a line shown at most. Laying out and drawing a line costs as much as it is long,
/// every frame it is in sight, and a minified file can be a single line of megabytes.
pub(crate) const MAX_LINE_CHARS: usize = 5000;

/// Cuts a line shown (and its colours) to [`MAX_LINE_CHARS`], saying how much was left out;
/// returns the line as it was when it was cut. Only what is shown: patches are made from the
/// line as git printed it.
pub(crate) fn cap_line(text: &mut String, spans: Option<&mut Spans>) -> Option<String> {
    if text.len() <= MAX_LINE_CHARS {
        return None;
    }
    let (cut, _) = text.char_indices().nth(MAX_LINE_CHARS)?;
    let note = format!(" … {} more characters", text[cut..].chars().count());
    let whole = text.clone();
    text.truncate(cut);
    if let Some(spans) = spans {
        let mut left = MAX_LINE_CHARS;
        spans.retain_mut(|(_, s)| {
            if left == 0 {
                return false;
            }
            match s.char_indices().nth(left) {
                Some((i, _)) => {
                    s.truncate(i);
                    left = 0;
                }
                None => left -= s.chars().count(),
            }
            true
        });
        spans.push((Color32::GRAY, note.clone()));
    }
    text.push_str(&note);
    Some(whole)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cached_lines_preserve_multiline_and_embedded_syntaxes() -> Result<(), String> {
        let cases = [
            (
                "rs",
                "// first\nfn a() {\n/*\nlet x = 1;\n*/\nlet x = 1;\nlet raw = r#\"\nlet x = 1;\n\"#;\n}\n",
            ),
            (
                "toml",
                "# first\n[[package]]\nname = \"example\"\ndependencies = [\n\"example\",\n]\ntext = '''\n[[package]]\nname = \"example\"\n'''\n",
            ),
            (
                "js",
                "// first\nfunction a() {\n/*\nconst x = 1;\n*/\nconst x = 1;\nconst raw = `\nconst x = 1;\n`;\n}\n",
            ),
            (
                "html",
                "<!-- first -->\n<div>example</div>\n<script>\nconst x = 1;\n</script>\n<style>\ndiv { color: red; }\n</style>\n",
            ),
        ];
        let ss = syntaxes();
        for (extension, block) in cases {
            let source = block.repeat(40);
            let lines: Vec<_> = source.lines().collect();
            let syntax = ss
                .find_syntax_by_extension(extension)
                .ok_or("missing syntax")?;
            for name in ["base16-ocean.dark", "InspiredGitHub"] {
                let theme = themes().themes.get(name).ok_or("missing theme")?;
                let actual =
                    colour_lines(lines.clone(), syntax, theme, ss).ok_or("highlight failed")?;
                let mut reference = syntect::easy::HighlightLines::new(syntax, theme);
                let expected: Result<Vec<Spans>, _> = lines
                    .iter()
                    .map(|line| {
                        let text = format!("{line}\n");
                        reference.highlight_line(&text, ss).map(|spans| {
                            spans
                                .into_iter()
                                .filter_map(|(style, text)| {
                                    let text = text.trim_end_matches('\n');
                                    let c = style.foreground;
                                    (!text.is_empty()).then(|| {
                                        (Color32::from_rgb(c.r, c.g, c.b), text.to_owned())
                                    })
                                })
                                .collect()
                        })
                    })
                    .collect();
                let expected = expected.map_err(|e| e.to_string())?;
                for (actual, expected) in actual.iter().zip(&expected) {
                    let colours = |spans: &Spans| -> Vec<_> {
                        spans
                            .iter()
                            .flat_map(|(c, s)| s.bytes().map(|b| (*c, b)))
                            .collect()
                    };
                    assert_eq!(colours(actual), colours(expected), "{extension}, {name}");
                }
            }
        }
        Ok(())
    }

    #[test]
    fn long_lines_are_cut_with_their_colours() {
        let red = Color32::RED;
        let mut short = "abc".to_owned();
        assert_eq!(cap_line(&mut short, None), None);
        assert_eq!(short, "abc");

        let mut text = "é".repeat(MAX_LINE_CHARS + 7);
        let mut spans: Spans = vec![
            (red, "é".repeat(MAX_LINE_CHARS - 1)),
            (red, "é".repeat(5)),
            (red, "é".repeat(3)),
        ];
        let whole = text.clone();
        assert_eq!(cap_line(&mut text, Some(&mut spans)), Some(whole));
        assert_eq!(
            text,
            format!("{} … 7 more characters", "é".repeat(MAX_LINE_CHARS))
        );
        let joined: String = spans.iter().map(|(_, s)| s.as_str()).collect();
        assert_eq!(joined, text);
        assert_eq!(spans.len(), 3);
    }

    #[test]
    fn colours_rust_and_skips_plain_text() {
        let lines = ["fn main() {", "    let x = 1;", "}"];
        let spans = highlight("a.rs", lines.iter().copied(), true);
        let spans = spans.unwrap_or_default();
        assert_eq!(spans.len(), 3);
        let text: String = spans[1].iter().map(|(_, t)| t.as_str()).collect();
        assert_eq!(text, "    let x = 1;");
        // More than one colour on a line of code.
        assert!(spans[0].len() > 1);
        assert!(highlight("notes.txt", lines.iter().copied(), true).is_none());
        assert!(highlight("Makefile-without-ext", lines.iter().copied(), true).is_none());
    }

    #[test]
    fn whole_names_win_over_extensions() {
        let lines = ["project(x)", "add_executable(x main.c)"];
        let spans = highlight("src/CMakeLists.txt", lines.iter().copied(), true);
        assert!(spans.is_some_and(|s| s.iter().any(|line| line.len() > 1)));
        assert!(highlight("MAIN.RS", ["fn main() {}"].into_iter(), true).is_some());
    }

    /// A minified file is a few very long lines: too slow to colour.
    #[test]
    fn very_long_text_stays_plain() {
        let line = "var a=1;".repeat(MAX_BYTES / 8 + 1);
        assert!(highlight("app.min.js", [line.as_str()].into_iter(), true).is_none());
        assert!(highlight("app.js", ["var a=1;"].into_iter(), true).is_some());
    }

    /// The languages of a typical web or .NET repository each get more than one colour.
    #[test]
    fn colours_common_languages() {
        let cases: &[(&str, &[&str])] = &[
            ("Program.cs", &["public class A { private int x = 1; }"]),
            ("app.js", &["const x = require('y');"]),
            ("app.ts", &["const x: number = 1;"]),
            ("App.tsx", &["export const A = (): JSX.Element => <div />;"]),
            (
                "Details.vue",
                &[
                    "<script setup lang=\"ts\">",
                    "import { ref } from 'vue'",
                    "</script>",
                ],
            ),
            ("Cargo.toml", &["[package]", "name = \"x\""]),
            ("Dockerfile", &["FROM alpine:3", "RUN echo hi"]),
            ("styles.scss", &[".a { color: red; }"]),
        ];
        let plain: Vec<&str> = cases
            .iter()
            .filter(|(path, lines)| {
                !highlight(path, lines.iter().copied(), true)
                    .is_some_and(|spans| spans.iter().any(|line| line.len() > 1))
            })
            .map(|(path, _)| *path)
            .collect();
        assert!(plain.is_empty(), "not coloured: {plain:?}");
    }
}
