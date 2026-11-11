//! Word-level changes: two texts cut into words and compared, as `git diff --word-diff` and
//! wdiff do. Whitespace only separates words, so a paragraph wrapped differently compares
//! equal.

/// Past this many pairs of words to compare, the middle that differs is shown removed then
/// added.
const MAX_CELLS: usize = 4_000_000;

/// What happened to a word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    Kept,
    Removed,
    Added,
}

/// The words of `text`: runs of letters and digits (with `'`, `-` and `_` inside), and any
/// other character that isn't whitespace on its own.
pub fn words(text: &str) -> Vec<&str> {
    spans(text).into_iter().map(|(s, e)| &text[s..e]).collect()
}

/// Where the [`words`] of `text` are, as byte ranges.
pub fn spans(text: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    let word_char = |c: char| c.is_alphanumeric() || matches!(c, '_' | '\'' | '’' | '-');
    for (i, c) in text.char_indices() {
        // Marks that combine with the letter before (accents, vowel signs, viramas) belong to
        // its word.
        if word_char(c) || (start.is_some() && combines(c)) {
            start.get_or_insert(i);
            continue;
        }
        if let Some(s) = start.take() {
            out.push((s, i));
        }
        // A byte order mark is no word.
        if !c.is_whitespace() && c != '\u{feff}' {
            out.push((i, i + c.len_utf8()));
        }
    }
    if let Some(s) = start {
        out.push((s, text.len()));
    }
    out
}

/// Whether `c` is a mark that combines with the character before it rather than a sign of its
/// own: not whitespace, not punctuation or a symbol.
fn combines(c: char) -> bool {
    !c.is_whitespace()
        && !c.is_ascii()
        && !matches!(c, '\u{2000}'..='\u{2BFF}' | '\u{3000}'..='\u{303F}' | '\u{FE30}'..='\u{FE4F}' | '\u{FF00}'..='\u{FF65}')
        && !c.is_alphanumeric()
        && c != '\u{feff}'
}

/// `old` against `new` word by word, each word with what stood before it in its own text
/// (the whitespace or nothing), so that the text reads as written: `e.g.` and links stay
/// whole. Line breaks become spaces, except before a list item or heading.
pub fn diff_text(old: &str, new: &str) -> Vec<(Change, String)> {
    let (os, ns) = (spans(old), spans(new));
    let ow: Vec<&str> = os.iter().map(|&(s, e)| &old[s..e]).collect();
    let nw: Vec<&str> = ns.iter().map(|&(s, e)| &new[s..e]).collect();
    let (mut i, mut j) = (0, 0);
    let piece = |text: &str, spans: &[(usize, usize)], k: usize| -> String {
        let (s, e) = spans[k];
        let before = if k == 0 { "" } else { &text[spans[k - 1].1..s] };
        format!("{}{}", gap(before, &text[s..]), &text[s..e])
    };
    let mut out = Vec::new();
    for (change, _) in diff(&ow, &nw) {
        match change {
            Change::Kept => {
                out.push((change, piece(new, &ns, j)));
                i += 1;
                j += 1;
            }
            Change::Removed => {
                out.push((change, piece(old, &os, i)));
                i += 1;
            }
            Change::Added => {
                out.push((change, piece(new, &ns, j)));
                j += 1;
            }
        }
    }
    out
}

/// What stands between two words, `rest` being the text from the second on: nothing, a
/// space, or a line break before a list item or heading.
fn gap(between: &str, rest: &str) -> &'static str {
    if between.is_empty() {
        ""
    } else if between.contains('\n') && starts_block(rest) {
        "\n"
    } else {
        " "
    }
}

/// Whether a line starting with `text` starts a block of its own: a list item, a heading, a
/// quote or a table row.
pub fn starts_block(text: &str) -> bool {
    let t = text.trim_start();
    t.starts_with(['-', '*', '+', '#', '|', '>'])
        || t.split_once(['.', ')']).is_some_and(|(n, _)| {
            !n.is_empty() && n.len() < 4 && n.chars().all(|c| c.is_ascii_digit())
        })
}

/// `old` against `new`, word by word, in order.
pub fn diff<'a>(old: &[&'a str], new: &[&'a str]) -> Vec<(Change, &'a str)> {
    let prefix = old.iter().zip(new).take_while(|(a, b)| a == b).count();
    let suffix = old[prefix..]
        .iter()
        .rev()
        .zip(new[prefix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let (a, b) = (
        &old[prefix..old.len() - suffix],
        &new[prefix..new.len() - suffix],
    );
    let mut out: Vec<(Change, &str)> = old[..prefix].iter().map(|w| (Change::Kept, *w)).collect();
    let (n, m) = (a.len(), b.len());
    if n.saturating_mul(m) > MAX_CELLS {
        out.extend(a.iter().map(|w| (Change::Removed, *w)));
        out.extend(b.iter().map(|w| (Change::Added, *w)));
    } else {
        let mut lcs = vec![vec![0u32; m + 1]; n + 1];
        for i in (0..n).rev() {
            for j in (0..m).rev() {
                lcs[i][j] = if a[i] == b[j] {
                    lcs[i + 1][j + 1] + 1
                } else {
                    lcs[i + 1][j].max(lcs[i][j + 1])
                };
            }
        }
        let (mut i, mut j) = (0, 0);
        while i < n || j < m {
            if i < n && j < m && a[i] == b[j] {
                out.push((Change::Kept, a[i]));
                i += 1;
                j += 1;
            } else if i < n && (j == m || lcs[i + 1][j] >= lcs[i][j + 1]) {
                out.push((Change::Removed, a[i]));
                i += 1;
            } else {
                out.push((Change::Added, b[j]));
                j += 1;
            }
        }
    }
    out.extend(old[old.len() - suffix..].iter().map(|w| (Change::Kept, *w)));
    out
}

/// How much of `old` and `new` is the same, from 0 to 1, by their words.
pub fn similarity(old: &[&str], new: &[&str]) -> f32 {
    let total = old.len() + new.len();
    if total == 0 {
        return 1.0;
    }
    // `diff` itself gives up on a middle too big to compare (after what both start and end
    // with), so a small edit to a long paragraph still compares.
    let kept = diff(old, new)
        .iter()
        .filter(|(c, _)| *c == Change::Kept)
        .count();
    (2 * kept) as f32 / total as f32
}
