//! Public declarations a change touched: a removed line that declared something public, not
//! added back as it was, is a public signature changed (or removed).

/// Whether `line` (trimmed) declares something public, in the usual languages.
pub fn is_public(path: &str, line: &str) -> bool {
    let ext = path.rsplit_once('.').map(|(_, e)| e).unwrap_or_default();
    let t = line.trim_start();
    match ext {
        "rs" => {
            let rest = t.strip_prefix("pub ").map(|r| {
                ["async ", "const ", "unsafe ", "extern \"C\" "]
                    .iter()
                    .fold(r, |r, q| r.strip_prefix(q).unwrap_or(r))
            });
            rest.is_some_and(|r| {
                [
                    "fn ", "struct ", "enum ", "trait ", "type ", "const ", "static ", "mod ",
                    "union ",
                ]
                .iter()
                .any(|k| r.starts_with(k))
            })
        }
        "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" => t.starts_with("export "),
        // Top level only (no indentation), not `_private`.
        "py" => {
            let top = !line.starts_with([' ', '\t']);
            let name = t
                .strip_prefix("def ")
                .or_else(|| t.strip_prefix("async def "))
                .or_else(|| t.strip_prefix("class "));
            top && name.is_some_and(|n| !n.starts_with('_'))
        }
        "go" => t.strip_prefix("func ").is_some_and(|r| {
            // `func Name(` or `func (r T) Name(`: exported when capitalised.
            let name = if r.starts_with('(') {
                r.split_once(") ").map_or("", |(_, n)| n)
            } else {
                r
            };
            name.chars().next().is_some_and(char::is_uppercase)
        }),
        "java" | "kt" | "cs" | "scala" | "swift" => {
            t.starts_with("public ")
                && (t.contains('(') || t.contains(" class ") || t.contains(" interface "))
        }
        _ => false,
    }
}

/// A public declaration a change took away or changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Signature {
    pub path: String,
    /// Its line in the old file.
    pub line: u32,
    pub text: String,
}

/// The public declarations `patch` (a `-U0` diff) removed and didn't add back unchanged.
pub fn changed(patch: &str) -> Vec<Signature> {
    let mut out = Vec::new();
    let mut path = String::new();
    let mut old = 0u32;
    let mut added: Vec<(String, String)> = Vec::new();
    // In a file's header (`---`, `+++`), until its first hunk.
    let mut header = false;
    for line in patch.lines() {
        if line.starts_with("diff --git ") {
            header = true;
        } else if header && let Some(p) = line.strip_prefix("+++ b/") {
            path = p.trim_end_matches('\t').to_owned();
        } else if header && let Some(p) = line.strip_prefix("--- a/") {
            path = p.trim_end_matches('\t').to_owned();
        } else if header && !line.starts_with("@@") {
        } else if line.starts_with("@@") {
            header = false;
            old = line
                .split_whitespace()
                .nth(1)
                .and_then(|p| p.strip_prefix('-'))
                .and_then(|p| p.split(',').next())
                .and_then(|n| n.parse().ok())
                .unwrap_or(0);
        } else if let Some(text) = line.strip_prefix('-') {
            if is_public(&path, text) {
                out.push(Signature {
                    path: path.clone(),
                    line: old,
                    text: text.trim().to_owned(),
                });
            }
            old += 1;
        } else if let Some(text) = line.strip_prefix('+') {
            added.push((path.clone(), text.trim().to_owned()));
        }
    }
    // The same declaration added back (moved, or re-indented): unchanged.
    out.retain(|s| !added.iter().any(|(p, t)| *p == s.path && *t == s.text));
    out
}
