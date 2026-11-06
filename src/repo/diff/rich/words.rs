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
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    let word_char = |c: char| c.is_alphanumeric() || matches!(c, '_' | '\'' | '’' | '-');
    for (i, c) in text.char_indices() {
        if word_char(c) {
            start.get_or_insert(i);
            continue;
        }
        if let Some(s) = start.take() {
            out.push(&text[s..i]);
        }
        if !c.is_whitespace() {
            out.push(&text[i..i + c.len_utf8()]);
        }
    }
    if let Some(s) = start {
        out.push(&text[s..]);
    }
    out
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
    if old.len().saturating_mul(new.len()) > MAX_CELLS {
        return 0.0;
    }
    let kept = diff(old, new)
        .iter()
        .filter(|(c, _)| *c == Change::Kept)
        .count();
    (2 * kept) as f32 / total as f32
}

/// Whether a space goes between `prev` and `word` when the words are put back together:
/// not before closing punctuation, nor after opening punctuation.
pub fn spaced(prev: &str, word: &str) -> bool {
    let closes = word
        .chars()
        .next()
        .is_some_and(|c| ".,;:!?)]}»”%".contains(c));
    let opens = prev.chars().last().is_some_and(|c| "([{«“".contains(c));
    !closes && !opens
}
