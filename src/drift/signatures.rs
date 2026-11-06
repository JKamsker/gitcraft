//! Public declarations a change touched: a removed line that declared something public, not
//! added back as it was, is a public signature changed (or removed).

/// Whether `line` (trimmed) declares something public, in the usual languages.
pub fn is_public(path: &str, line: &str) -> bool {
    let ext = path.rsplit_once('.').map(|(_, e)| e).unwrap_or_default();
    let t = line.trim_start();
    match ext {
        "rs" => t.strip_prefix("pub ").is_some_and(rust_item),
        // Functions, classes and types; an exported constant only when it holds a function
        // (a value changed is no signature changed).
        "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" => t.strip_prefix("export ").is_some_and(|r| {
            let r = r.strip_prefix("default ").unwrap_or(r);
            let r = r.strip_prefix("declare ").unwrap_or(r);
            let typed = [
                "function",
                "async function",
                "class",
                "abstract class",
                "interface",
                "type ",
                "enum ",
            ]
            .iter()
            .any(|k| r.starts_with(k));
            let holds_function = (r.starts_with("const ") || r.starts_with("let "))
                && (r.contains("=>") || r.contains("function"));
            typed || holds_function
        }),
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
        // A method (its `(` before any `=`), a class, an interface or a record.
        "java" | "kt" | "cs" | "scala" | "swift" => {
            let method = match (t.find('('), t.find('=')) {
                (Some(p), Some(e)) => p < e,
                (Some(_), None) => true,
                _ => false,
            };
            let kind = [" class ", " interface ", " record ", " enum "]
                .iter()
                .any(|k| t.contains(k));
            t.starts_with("public ") && (method || kind)
        }
        _ => false,
    }
}

/// What follows `pub ` declares an item: `fn`, `struct`…, a `const` / `static`, a function
/// with its qualifiers (`const fn`, `async unsafe fn`, `extern "C" fn`).
fn rust_item(rest: &str) -> bool {
    let mut r = rest;
    loop {
        let next = ["async ", "unsafe ", "extern \"C\" "]
            .iter()
            .find_map(|q| r.strip_prefix(q))
            .or_else(|| {
                r.strip_prefix("const ").filter(|x| {
                    ["fn ", "unsafe ", "async "]
                        .iter()
                        .any(|k| x.starts_with(k))
                })
            });
        match next {
            Some(n) => r = n,
            None => break,
        }
    }
    [
        "fn ", "struct ", "enum ", "trait ", "type ", "const ", "static ", "mod ", "union ",
    ]
    .iter()
    .any(|k| r.starts_with(k))
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
        } else if header && let Some(p) = side(line, "+++ ", "b/") {
            path = p;
        } else if header && let Some(p) = side(line, "--- ", "a/") {
            path = p;
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

/// The path of a `--- a/x` or `+++ b/x` header line (quoted ones too: `"b/tab\there"`).
fn side(line: &str, mark: &str, prefix: &str) -> Option<String> {
    let rest = line.strip_prefix(mark)?.trim_end_matches('\t');
    let rest = rest
        .strip_prefix('"')
        .and_then(|r| r.strip_suffix('"'))
        .map(|r| {
            r.replace("\\t", "\t")
                .replace("\\\"", "\"")
                .replace("\\\\", "\\")
        })
        .unwrap_or_else(|| rest.to_owned());
    rest.strip_prefix(prefix).map(str::to_owned)
}
