//! Dependencies a manifest names, to tell those a change added: Cargo, npm, Python, Go.

/// Whether `path` is a manifest whose dependencies are read.
pub fn is_manifest(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    matches!(
        name,
        "Cargo.toml" | "package.json" | "pyproject.toml" | "go.mod" | "Gemfile" | "composer.json"
    ) || (name.ends_with(".txt")
        && (name.starts_with("requirements") || path.contains("requirements/")))
}

/// `line` without its `#` comment (one outside quotes).
fn uncommented(line: &str) -> &str {
    let mut quote: Option<char> = None;
    for (i, c) in line.char_indices() {
        match (quote, c) {
            (None, '"' | '\'') => quote = Some(c),
            (Some(q), c) if c == q => quote = None,
            (None, '#') => return &line[..i],
            _ => {}
        }
    }
    line
}

/// The dependencies the manifest `path` with `text` names.
pub fn names(path: &str, text: &str) -> Vec<String> {
    let name = path.rsplit('/').next().unwrap_or(path);
    let mut out = match name {
        "Cargo.toml" => toml_tables(text, |t| t.ends_with("dependencies")),
        "pyproject.toml" => pyproject(text),
        "package.json" | "composer.json" => json(text),
        "go.mod" => go_mod(text),
        "Gemfile" => gemfile(text),
        _ => requirements(text),
    };
    out.sort();
    out.dedup();
    out
}

/// The keys of the TOML tables `wanted` holds for (`[dependencies]`,
/// `[target.'cfg(x)'.dev-dependencies]`, `[dependencies.serde]`…): `serde = "1"`,
/// `serde.workspace = true`, `serde = { version = "1",` spread over lines.
fn toml_tables(text: &str, wanted: impl Fn(&str) -> bool) -> Vec<String> {
    let mut out = Vec::new();
    let mut inside = false;
    // How deep inside a value spread over lines (`{ …` / `[ …` not closed yet).
    let mut depth: i32 = 0;
    for line in text.lines().map(|l| uncommented(l).trim()) {
        if depth > 0 {
            depth += balance(line);
            continue;
        }
        if let Some(table) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            let table = table.trim_matches(['[', ']']).trim();
            // `[dependencies.serde]`: the table is one dependency.
            if let Some((parent, dep)) = table.rsplit_once('.')
                && wanted(parent)
            {
                out.push(dep.trim_matches(['"', '\'']).to_owned());
                inside = false;
                continue;
            }
            inside = wanted(table);
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            depth = balance(value).max(0);
            if inside {
                let key = key.trim();
                let name = key
                    .strip_prefix('"')
                    .and_then(|k| k.split_once('"'))
                    .map(|(n, _)| n)
                    .unwrap_or_else(|| key.split('.').next().unwrap_or(key));
                out.push(name.trim_matches(['"', '\'']).to_owned());
            }
        }
    }
    out.retain(|k| !k.is_empty());
    out
}

/// How many more `{` / `[` than `}` / `]` a line opens (outside quotes).
fn balance(line: &str) -> i32 {
    let mut quote: Option<char> = None;
    let mut n = 0;
    for c in line.chars() {
        match (quote, c) {
            (None, '"' | '\'') => quote = Some(c),
            (Some(q), c) if c == q => quote = None,
            (None, '{' | '[') => n += 1,
            (None, '}' | ']') => n -= 1,
            _ => {}
        }
    }
    n
}

/// `[project] dependencies = [...]` (PEP 621), its extras (`[project.optional-dependencies]`),
/// dependency groups (PEP 735) and Poetry's tables.
fn pyproject(text: &str) -> Vec<String> {
    let mut out = toml_tables(text, |t| {
        t.starts_with("tool.poetry") && t.ends_with("dependencies")
    });
    out.retain(|n| n != "python");
    let mut table = String::new();
    let mut in_list = false;
    for line in text.lines().map(|l| uncommented(l).trim()) {
        if let Some(t) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            table = t.trim().to_owned();
            in_list = false;
            continue;
        }
        let lists = table == "project.optional-dependencies" || table == "dependency-groups";
        let starts = (table == "project" && line.starts_with("dependencies") && line.contains('['))
            || (lists && line.contains('=') && line.contains('['));
        if starts {
            in_list = true;
        }
        if in_list {
            let values = line.split_once('[').map_or(line, |(_, v)| v);
            let values = if starts { values } else { line };
            for quoted in values.split(['"', '\'']).skip(1).step_by(2) {
                out.push(requirement_name(quoted));
            }
            if line.contains(']') {
                in_list = false;
            }
        }
    }
    out.retain(|n| !n.is_empty());
    out
}

/// A requirement's name: `requests>=2; python_version<"3.8"` → `requests`.
fn requirement_name(spec: &str) -> String {
    spec.trim()
        .split(|c: char| !(c.is_alphanumeric() || matches!(c, '-' | '_' | '.')))
        .next()
        .unwrap_or_default()
        .to_lowercase()
}

fn requirements(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#') && !l.starts_with('-'))
        .filter_map(|l| {
            // `git+https://…#egg=name`: its name is the egg's; another URL names none.
            if let Some((_, egg)) = l.split_once("#egg=") {
                return Some(requirement_name(egg));
            }
            (!l.contains("://")).then(|| requirement_name(uncommented(l)))
        })
        .filter(|n| !n.is_empty())
        .collect()
}

/// Every key of the `*dependencies` objects of a package.json / composer.json (`require` too).
fn json(text: &str) -> Vec<String> {
    let Ok(serde_json::Value::Object(root)) = serde_json::from_str(text) else {
        return Vec::new();
    };
    root.iter()
        .filter(|(k, _)| {
            k.to_lowercase().ends_with("dependencies")
                || k.as_str() == "require"
                || k.as_str() == "require-dev"
        })
        .filter_map(|(_, v)| v.as_object())
        .flat_map(|deps| deps.keys().cloned())
        .collect()
}

fn go_mod(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut block = false;
    for line in text.lines().map(str::trim) {
        if line.starts_with("require (") {
            block = true;
            continue;
        }
        if block && line == ")" {
            block = false;
            continue;
        }
        let spec = if block {
            Some(line)
        } else {
            line.strip_prefix("require ")
        };
        if let Some(module) = spec.and_then(|s| s.split_whitespace().next())
            && !module.starts_with("//")
        {
            out.push(module.to_owned());
        }
    }
    out
}

fn gemfile(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter_map(|l| l.strip_prefix("gem "))
        .filter_map(|rest| rest.split(['"', '\'']).nth(1))
        .map(str::to_owned)
        .collect()
}

/// The dependencies `after` names that `before` doesn't.
pub fn added(path: &str, before: &str, after: &str) -> Vec<String> {
    let old = names(path, before);
    names(path, after)
        .into_iter()
        .filter(|n| !old.contains(n))
        .collect()
}
