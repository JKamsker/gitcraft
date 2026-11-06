//! Dependencies a manifest names, to tell those a change added: Cargo, npm, Python, Go.

/// Whether `path` is a manifest whose dependencies are read.
pub fn is_manifest(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    matches!(
        name,
        "Cargo.toml" | "package.json" | "pyproject.toml" | "go.mod" | "Gemfile" | "composer.json"
    ) || (name.starts_with("requirements") && name.ends_with(".txt"))
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
/// `[target.'cfg(x)'.dev-dependencies]`, `[dependencies.serde]`…).
fn toml_tables(text: &str, wanted: impl Fn(&str) -> bool) -> Vec<String> {
    let mut out = Vec::new();
    let mut inside = false;
    for line in text.lines().map(str::trim) {
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
        if inside
            && !line.starts_with('#')
            && let Some((key, _)) = line.split_once('=')
        {
            out.push(key.trim().trim_matches(['"', '\'']).to_owned());
        }
    }
    out.retain(|k| !k.is_empty());
    out
}

/// `[project] dependencies = [...]` (PEP 621) and Poetry's tables.
fn pyproject(text: &str) -> Vec<String> {
    let mut out = toml_tables(text, |t| {
        t.starts_with("tool.poetry") && t.ends_with("dependencies")
    });
    out.retain(|n| n != "python");
    let mut in_list = false;
    for line in text.lines().map(str::trim) {
        if line.starts_with("dependencies") && line.contains('[') {
            in_list = true;
        }
        if in_list {
            for quoted in line.split(['"', '\'']).skip(1).step_by(2) {
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
        .map(requirement_name)
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
