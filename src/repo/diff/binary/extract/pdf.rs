use super::*;

/// The text a PDF shows, as far as it is plainly in it: the strings of the text operators
/// (`Tj`, `TJ`, `'`, `"`) of its content streams, inflated when compressed, one stream at a
/// time within [`PDF_BUDGET`]. Fonts with their own encodings (much of what is made today)
/// come out as nothing, and then there is no text.
#[cfg(test)]
pub fn pdf_text(bytes: &[u8]) -> Option<String> {
    pdf_text_with_gaps(bytes, &mut Vec::new())
}

/// [`pdf_text`], saying in `gaps` when streams were left unread for the budget.
pub(super) fn pdf_text_with_gaps(bytes: &[u8], gaps: &mut Vec<String>) -> Option<String> {
    let mut text = String::new();
    let mut budget = PDF_BUDGET;
    let exhausted = each_content_stream(bytes, &mut budget, |stream| {
        let ops = text_operators(stream);
        if !ops.trim().is_empty() {
            text.push_str(&ops);
            text.push('\n');
        }
        text.len() < MAX_TEXT
    });
    if exhausted {
        gaps.push(crate::i18n::message(
            "repo-core-3873",
            &[(
                "PDF-BUDGET",
                format!("{}", PDF_BUDGET / (1024 * 1024)).into(),
            )],
        ));
    }
    let text = text.trim().to_owned();
    // Mostly odd characters: an encoding of its own, not text.
    let readable = text
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .count();
    (!text.is_empty() && readable * 10 >= text.chars().count() * 7).then_some(text)
}

/// Calls `each` with every content stream of a PDF (`stream … endstream`, inflated when its
/// dictionary says FlateDecode), until it says stop or `budget` (bytes inflated) runs out.
/// Pictures, fonts, cross-reference and object streams are left out. `true` when the budget
/// ran out with a stream left unread.
pub(super) fn each_content_stream(
    bytes: &[u8],
    budget: &mut usize,
    mut each: impl FnMut(&[u8]) -> bool,
) -> bool {
    const SKIP: [&str; 7] = [
        "/Image",
        "/Length1",
        "/FontFile",
        "/XRef",
        "/ObjStm",
        "/Subtype",
        "/Metadata",
    ];
    let mut at = 0;
    while let Some(i) = find(&bytes[at..], b"stream") {
        let start_kw = at + i;
        let mut start = start_kw + 6;
        if bytes.get(start) == Some(&b'\r') {
            start += 1;
        }
        if bytes.get(start) == Some(&b'\n') {
            start += 1;
        }
        // `endstream` itself matched `stream`: the next search starts past it.
        if start_kw >= 3 && &bytes[start_kw - 3..start_kw] == b"end" {
            at = start_kw + 6;
            continue;
        }
        // Its own dictionary: from the last `obj` before it, within reach.
        let window = &bytes[start_kw.saturating_sub(1024)..start_kw];
        let dict = window
            .windows(3)
            .rposition(|w| w == b"obj")
            .map_or(window, |i| &window[i..]);
        let dict = String::from_utf8_lossy(dict);
        // Its end: where its `/Length` says, when that is a number and `endstream` follows
        // there (the data may hold the word itself); else the next `endstream`.
        let declared = direct_length(&dict).filter(|&n| {
            bytes.get(start..).is_some_and(|rest| {
                rest.get(n..).is_some_and(|after| {
                    let ws = after.iter().take_while(|c| c.is_ascii_whitespace()).count();
                    after[ws..].starts_with(b"endstream")
                })
            })
        });
        let Some(len) = declared.or_else(|| find(&bytes[start..], b"endstream")) else {
            break;
        };
        let data = &bytes[start..start + len];
        at = start + len;
        if SKIP.iter().any(|k| dict.contains(k)) {
            continue;
        }
        let go_on = if dict.contains("FlateDecode") {
            if *budget == 0 {
                return true;
            }
            match miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(data, *budget) {
                Ok(inflated) => {
                    *budget = budget.saturating_sub(inflated.len().max(1));
                    each(&inflated)
                }
                // More than the budget left: that stream is past it.
                Err(e) if e.status == miniz_oxide::inflate::TINFLStatus::HasMoreOutput => {
                    return true;
                }
                Err(_) => true,
            }
        } else if !dict.contains("Decode") {
            each(data)
        } else {
            true
        };
        if !go_on {
            return false;
        }
    }
    false
}

/// A stream dictionary's `/Length` when it is a number (not a reference, `12 0 R`).
pub(super) fn direct_length(dict: &str) -> Option<usize> {
    fn number(s: &str) -> (&str, &str) {
        let s = s.trim_start();
        let end = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
        s.split_at(end)
    }
    let mut rest = dict;
    while let Some(i) = rest.find("/Length") {
        rest = &rest[i + 7..];
        // `/Length1` and such are other keys.
        if rest.starts_with(|c: char| c.is_ascii_alphanumeric()) {
            continue;
        }
        let (n, after) = number(rest);
        let n = n.parse().ok()?;
        let (generation, after) = number(after);
        let reference = !generation.is_empty() && after.trim_start().starts_with('R');
        return (!reference).then_some(n);
    }
    None
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// The strings shown by a content stream's text operators, a line per `BT … ET` block or
/// line move; in a `TJ` array, a large gap between strings is a space.
pub(super) fn text_operators(stream: &[u8]) -> String {
    let mut out = String::new();
    // The strings since the last operator, one after the other, kept to [`MAX_TEXT`]: a `TJ`
    // of millions of strings holds no more than that.
    let mut strings = String::new();
    let mut pending = false;
    let add = |strings: &mut String, s: &str| {
        if strings.len() + s.len() <= MAX_TEXT {
            strings.push_str(s);
        }
    };
    let mut i = 0;
    while i < stream.len() {
        match stream[i] {
            b'(' => {
                let (s, end) = literal_string(stream, i + 1);
                add(&mut strings, &s);
                pending = true;
                i = end;
            }
            b'<' if stream.get(i + 1) != Some(&b'<') => {
                let end = stream[i..]
                    .iter()
                    .position(|&b| b == b'>')
                    .map_or(stream.len(), |p| i + p);
                add(&mut strings, &hex_string(&stream[i + 1..end]));
                pending = true;
                i = end + 1;
            }
            b'%' => {
                while i < stream.len() && stream[i] != b'\n' {
                    i += 1;
                }
            }
            b'-' if pending => {
                // A kerning number in a TJ array: past this much, a gap between words.
                let start = i;
                i += 1;
                while i < stream.len() && (stream[i].is_ascii_digit() || stream[i] == b'.') {
                    i += 1;
                }
                let n: f32 = std::str::from_utf8(&stream[start..i])
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0.0);
                if n < -200.0 {
                    add(&mut strings, " ");
                }
                continue;
            }
            c if c.is_ascii_alphabetic() || c == b'\'' || c == b'"' => {
                let start = i;
                while i < stream.len()
                    && (stream[i].is_ascii_alphabetic() || matches!(stream[i], b'\'' | b'"' | b'*'))
                {
                    i += 1;
                }
                match &stream[start..i] {
                    b"Tj" | b"TJ" | b"'" | b"\"" => out.push_str(&strings),
                    b"Td" | b"TD" | b"T*" | b"ET" if !out.ends_with('\n') && !out.is_empty() => {
                        out.push('\n');
                    }
                    _ => {}
                }
                strings.clear();
                pending = false;
                if out.len() > MAX_TEXT {
                    break;
                }
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    out
}

/// A PDF literal string from after its `(`: the text and where it ends.
fn literal_string(b: &[u8], mut i: usize) -> (String, usize) {
    let mut out = Vec::new();
    let mut depth = 1;
    while i < b.len() {
        match b[i] {
            b'\\' => {
                i += 1;
                match b.get(i) {
                    Some(b'n') => out.push(b'\n'),
                    Some(b't') => out.push(b'\t'),
                    Some(b'r' | b'\n') => {}
                    Some(b'0'..=b'7') => {
                        let digits: Vec<u8> = b[i..]
                            .iter()
                            .take(3)
                            .take_while(|c| (b'0'..=b'7').contains(c))
                            .copied()
                            .collect();
                        let n = digits.iter().fold(0u32, |n, d| n * 8 + u32::from(d - b'0'));
                        out.push(n as u8);
                        i += digits.len() - 1;
                    }
                    Some(&c) => out.push(c),
                    None => {}
                }
            }
            b'(' => {
                depth += 1;
                out.push(b'(');
            }
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return (latin1(&out), i + 1);
                }
                out.push(b')');
            }
            c => out.push(c),
        }
        i += 1;
    }
    (latin1(&out), i)
}

/// PDF strings in the standard encodings read near enough as Latin-1; UTF-16 when marked.
pub fn latin1(b: &[u8]) -> String {
    if b.starts_with(&[0xFE, 0xFF]) {
        let units: Vec<u16> = b[2..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&c| u16::from_be_bytes(c))
            .collect();
        return String::from_utf16_lossy(&units);
    }
    b.iter().map(|&c| char::from(c)).collect()
}

fn hex_string(h: &[u8]) -> String {
    let digits: Vec<u8> = h.iter().copied().filter(u8::is_ascii_hexdigit).collect();
    let bytes: Vec<u8> = digits
        .chunks(2)
        .filter_map(|p| {
            let s = std::str::from_utf8(p).ok()?;
            u8::from_str_radix(&format!("{s:0<2}"), 16).ok()
        })
        .collect();
    latin1(&bytes)
}
