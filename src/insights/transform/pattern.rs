//! Telling edits apart by what they do: each side's text as tokens (whitespace dropped), the
//! names, numbers and strings both sides share turned into placeholders, so that the same
//! substitution made on different names has the same signature.

use std::collections::HashMap;

/// What a token is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Word,
    Number,
    Text,
    Punct,
}

/// A token, equal to another of the same kind and text wherever it is.
#[derive(Clone, Debug)]
pub struct Token {
    pub kind: Kind,
    pub text: String,
    /// Whitespace (or a line break) came before it: kept only to show it as it was written.
    pub spaced: bool,
}

impl PartialEq for Token {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind && self.text == other.text
    }
}

impl Eq for Token {}

impl Token {
    /// A name, number or string: what a placeholder may stand for.
    pub fn variable(&self) -> bool {
        self.kind != Kind::Punct
    }
}

/// Operators kept whole rather than split into their characters.
const OPERATORS: [&str; 12] = [
    "::", "->", "=>", "==", "!=", "<=", ">=", "&&", "||", "+=", "-=", "..",
];

/// The tokens of `text`, whitespace dropped.
pub fn tokenize(text: &str, out: &mut Vec<Token>) {
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    // A line break counts as whitespace.
    let mut gap = !out.is_empty();
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            gap = true;
            i += 1;
            continue;
        }
        let start = i;
        let kind = if c.is_alphabetic() || c == '_' {
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            Kind::Word
        } else if c.is_ascii_digit() {
            while i < chars.len() && (chars[i].is_alphanumeric() || matches!(chars[i], '_' | '.')) {
                i += 1;
            }
            Kind::Number
        } else if c == '"' || c == '`' {
            i += 1;
            while i < chars.len() && chars[i] != c {
                i += if chars[i] == '\\' { 2 } else { 1 };
            }
            i = (i + 1).min(chars.len());
            Kind::Text
        } else {
            let two: String = chars[i..(i + 2).min(chars.len())].iter().collect();
            i += if OPERATORS.contains(&two.as_str()) {
                2
            } else {
                1
            };
            Kind::Punct
        };
        out.push(Token {
            kind,
            text: chars[start..i].iter().collect(),
            spaced: gap,
        });
        gap = false;
    }
}

/// A token of a signature: itself, or the placeholder of what both sides share.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Sig {
    Lit(String),
    Hole(usize),
}

/// One side of an edit with what both sides share abstracted, and what each placeholder stood
/// for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Signature {
    pub old: Vec<Sig>,
    pub new: Vec<Sig>,
    pub bindings: Vec<String>,
}

impl Signature {
    /// The key edits doing the same thing share.
    pub fn key(&self) -> String {
        let mut key = String::new();
        for (side, toks) in [("", &self.old), ("\u{3}", &self.new)] {
            key.push_str(side);
            for t in toks {
                match t {
                    Sig::Lit(s) => key.push_str(s),
                    Sig::Hole(h) => {
                        key.push('\u{2}');
                        key.push_str(&h.to_string());
                    }
                }
                key.push('\u{1}');
            }
        }
        key
    }
}

/// `old` and `new` with the names, numbers and strings found on both sides made placeholders,
/// numbered in the order they come.
pub fn signature(old: &[Token], new: &[Token]) -> Signature {
    let in_new: std::collections::HashSet<&str> = new
        .iter()
        .filter(|t| t.variable())
        .map(|t| t.text.as_str())
        .collect();
    let in_old: std::collections::HashSet<&str> = old
        .iter()
        .filter(|t| t.variable())
        .map(|t| t.text.as_str())
        .collect();
    let mut holes: HashMap<String, usize> = HashMap::new();
    let mut bindings = Vec::new();
    let mut abstracted = |toks: &[Token], other: &std::collections::HashSet<&str>| -> Vec<Sig> {
        toks.iter()
            .map(|t| {
                if t.variable() && other.contains(t.text.as_str()) {
                    let next = holes.len();
                    let hole = *holes.entry(t.text.clone()).or_insert_with(|| {
                        bindings.push(t.text.clone());
                        next
                    });
                    Sig::Hole(hole)
                } else {
                    Sig::Lit(t.text.clone())
                }
            })
            .collect()
    };
    let old_sig = abstracted(old, &in_new);
    let new_sig = abstracted(new, &in_old);
    Signature {
        old: old_sig,
        new: new_sig,
        bindings,
    }
}

/// A token of a pattern's template: fixed text, or a placeholder (`$a`, `$b`…).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Part {
    Lit(String),
    Var(usize),
}

/// The template of a group of edits sharing a signature: placeholders bound to the same text
/// in every edit are that text; the others stay placeholders, renumbered.
pub fn template(sig: &Signature, all_bindings: &[&[String]]) -> (Vec<Part>, Vec<Part>) {
    let fixed: Vec<bool> = (0..sig.bindings.len())
        .map(|h| all_bindings.iter().all(|b| b.get(h) == sig.bindings.get(h)))
        .collect();
    let mut names: HashMap<usize, usize> = HashMap::new();
    let mut side = |toks: &[Sig]| -> Vec<Part> {
        toks.iter()
            .map(|t| match t {
                Sig::Lit(s) => Part::Lit(s.clone()),
                Sig::Hole(h) if fixed.get(*h).copied().unwrap_or(false) => {
                    Part::Lit(sig.bindings[*h].clone())
                }
                Sig::Hole(h) => {
                    let next = names.len();
                    Part::Var(*names.entry(*h).or_insert(next))
                }
            })
            .collect()
    };
    let old = side(&sig.old);
    let new = side(&sig.new);
    (old, new)
}

/// Whether `toks` are what `template` describes: its text where it has text, and each
/// placeholder a name, number or string, the same one wherever the placeholder recurs.
pub fn matches(template: &[Part], toks: &[Token]) -> bool {
    if template.len() != toks.len() {
        return false;
    }
    let mut bound: HashMap<usize, &str> = HashMap::new();
    template.iter().zip(toks).all(|(part, tok)| match part {
        Part::Lit(s) => *s == tok.text,
        Part::Var(v) => tok.variable() && *bound.entry(*v).or_insert(&tok.text) == tok.text,
    })
}

/// The tokens that differ once what both sides start and end with is cut off.
pub fn core(sig: &Signature) -> (Vec<Sig>, Vec<Sig>) {
    let (old, new) = (&sig.old, &sig.new);
    let prefix = old.iter().zip(new).take_while(|(a, b)| a == b).count();
    let room = old.len().min(new.len()) - prefix;
    let suffix = old
        .iter()
        .rev()
        .zip(new.iter().rev())
        .take(room)
        .take_while(|(a, b)| a == b)
        .count();
    (
        old[prefix..old.len() - suffix].to_vec(),
        new[prefix..new.len() - suffix].to_vec(),
    )
}

/// How alike two token lists are, 0 to 1 (Dice's coefficient of their multisets); two empty
/// lists are alike.
pub fn dice(a: &[Sig], b: &[Sig]) -> f32 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    let mut counts: HashMap<&Sig, i32> = HashMap::new();
    for t in a {
        *counts.entry(t).or_default() += 1;
    }
    let mut shared = 0;
    for t in b {
        if let Some(n) = counts.get_mut(t)
            && *n > 0
        {
            *n -= 1;
            shared += 1;
        }
    }
    2.0 * shared as f32 / (a.len() + b.len()) as f32
}

/// A template as text, spaced as `like` (the tokens of one of its edits) was written; as code
/// usually is when they don't line up.
pub fn render_like(parts: &[Part], like: &[Token]) -> String {
    if parts.len() != like.len() {
        return render(parts);
    }
    let mut out = String::new();
    for (i, (part, tok)) in parts.iter().zip(like).enumerate() {
        if i > 0 && tok.spaced {
            out.push(' ');
        }
        match part {
            Part::Lit(s) => out.push_str(s),
            Part::Var(v) => {
                out.push('$');
                out.push_str(&var_name(*v));
            }
        }
    }
    out
}

/// A template as text: `foo($a, ctx)`, spaced as code usually is.
pub fn render(parts: &[Part]) -> String {
    let mut out = String::new();
    let mut prev: Option<&str> = None;
    for part in parts {
        let text = match part {
            Part::Lit(s) => s.clone(),
            Part::Var(v) => format!("${}", var_name(*v)),
        };
        if let Some(p) = prev
            && spaced(p, &text)
        {
            out.push(' ');
        }
        out.push_str(&text);
        prev = Some(match part {
            Part::Lit(s) => s.as_str(),
            Part::Var(_) => "$",
        });
    }
    out
}

fn var_name(v: usize) -> String {
    let letters = b"abcdefghijklmnopqrstuvwxyz";
    if v < letters.len() {
        (letters[v] as char).to_string()
    } else {
        format!("v{v}")
    }
}

/// Whether a space goes between tokens `a` and `b`.
fn spaced(a: &str, b: &str) -> bool {
    let word = |s: &str| {
        s.chars()
            .next()
            .is_some_and(|c| c.is_alphanumeric() || matches!(c, '_' | '$' | '"' | '`'))
    };
    if matches!(b, "," | ";" | ")" | "]" | "." | "::" | "?" | ":" | ">") {
        return false;
    }
    if matches!(a, "(" | "[" | "." | "::" | "!" | "&" | "#" | "<" | "@") {
        return false;
    }
    if matches!(b, "(" | "[" | "<") {
        let keyword = matches!(
            a,
            "if" | "while" | "for" | "match" | "return" | "switch" | "in"
        );
        return keyword || !(word(a) || matches!(a, ")" | "]" | ">"));
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(s: &str) -> Vec<Token> {
        let mut out = Vec::new();
        tokenize(s, &mut out);
        out
    }

    #[test]
    fn tokens_drop_whitespace_and_keep_strings_and_operators_whole() {
        let t = toks("  let x  = foo::bar(\"a b\", 1.5) -> y;");
        let texts: Vec<&str> = t.iter().map(|t| t.text.as_str()).collect();
        assert_eq!(
            texts,
            [
                "let", "x", "=", "foo", "::", "bar", "(", "\"a b\"", ",", "1.5", ")", "->", "y",
                ";"
            ]
        );
        assert_eq!(t[7].kind, Kind::Text);
        assert_eq!(t[9].kind, Kind::Number);
        assert_eq!(toks("a  (b)"), toks("a(b)"));
    }

    #[test]
    fn shared_names_become_placeholders_and_the_same_edit_on_others_shares_a_key() {
        let a = signature(&toks("foo(x)"), &toks("foo(x, ctx)"));
        let b = signature(&toks("foo(y)"), &toks("foo(y, ctx)"));
        let c = signature(&toks("foo(y)"), &toks("foo(y, None)"));
        assert_eq!(a.key(), b.key());
        assert_ne!(a.key(), c.key());
        assert_eq!(a.bindings, ["foo", "x"]);
        let (old, new) = template(&a, &[&a.bindings, &b.bindings]);
        assert_eq!(render(&old), "foo($a)");
        assert_eq!(render(&new), "foo($a, ctx)");
        // The deviating edit fits the pattern's before side.
        assert!(matches(&old, &toks("foo(z)")));
        assert!(!matches(&old, &toks("bar(z)")));
        assert!(!matches(&old, &toks("foo(z, w)")));
    }

    #[test]
    fn the_core_is_what_differs_and_dice_measures_likeness() {
        let s = signature(&toks("foo(x)"), &toks("foo(x, ctx)"));
        let (old, new) = core(&s);
        assert!(old.is_empty());
        assert_eq!(new, [Sig::Lit(",".into()), Sig::Lit("ctx".into())]);
        assert_eq!(dice(&[], &[]), 1.0);
        assert_eq!(dice(&new, &new), 1.0);
        assert_eq!(dice(&new, &[]), 0.0);
    }

    #[test]
    fn templates_read_like_code() {
        let parts =
            |s: &str| -> Vec<Part> { toks(s).into_iter().map(|t| Part::Lit(t.text)).collect() };
        assert_eq!(render(&parts("a.b(c, d)[0];")), "a.b(c, d)[0];");
        assert_eq!(render(&parts("let x: Vec<u8> = y;")), "let x: Vec<u8> = y;");
        assert_eq!(render(&parts("if (a && !b)")), "if (a && !b)");
        // Spaced as written, when the tokens are at hand.
        let written = toks("/// it's |ui| {");
        let like = parts("/// it's |ui| {");
        assert_eq!(render_like(&like, &written), "/// it's |ui| {");
        let s = signature(&toks("foo( x )"), &toks("foo(x,  ctx)"));
        let (old, new) = template(&s, &[&s.bindings]);
        assert_eq!(render_like(&old, &toks("foo( x )")), "foo( x )");
        assert_eq!(render_like(&new, &toks("foo(x,  ctx)")), "foo(x, ctx)");
    }
}
