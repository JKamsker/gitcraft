//! Tests of config files by key: the readers (lines, errors, odd input) and the comparison.

use super::config::{self, Change, Entry};
use super::tree::{Node, Ty};
use super::*;

fn sides(old: &str, new: &str) -> Sides {
    Sides {
        old: Some(old.to_owned()),
        new: Some(new.to_owned()),
    }
}

fn entries(path: &str, old: &str, new: &str) -> Vec<Entry> {
    config::diff(path, &sides(old, new))
        .map(|d| d.entries)
        .unwrap_or_else(|e| {
            vec![Entry {
                path: e,
                change: Change::Changed,
                old: None,
                new: None,
                kinds: None,
                old_line: None,
                new_line: None,
            }]
        })
}

fn line_of(node: &Node, key: &str) -> Option<u32> {
    match node {
        Node::Map(fields) => fields.iter().find(|f| f.key == key).map(|f| f.line),
        _ => None,
    }
}

#[test]
fn json_keeps_the_line_of_each_key() -> Result<(), String> {
    let text = "{\n  \"a\": 1,\n  \"b\": {\n    \"c\": \"x\\u00e9\\ud83d\\ude00\"\n  },\n  \"d\": [true, null]\n}\n";
    let node = json::parse(text)?;
    assert_eq!(line_of(&node, "a"), Some(2));
    assert_eq!(line_of(&node, "d"), Some(6));
    let Node::Map(fields) = &node else {
        return Err("not a map".to_owned());
    };
    let Node::Map(inner) = &fields[1].node else {
        return Err("b not a map".to_owned());
    };
    assert_eq!(
        inner[0].node,
        Node::Scalar {
            text: "xé😀".to_owned(),
            ty: Ty::String
        }
    );
    // Comments and trailing commas of JSON-with-comments files, and a byte order mark.
    assert!(json::parse("\u{feff}{ // c\n \"a\": [1, 2,], /* x */ }").is_ok());
    Ok(())
}

#[test]
fn malformed_json_says_where() {
    assert_eq!(
        json::parse("{\n \"a\": 1\n \"b\": 2}"),
        Err("line 3: expected `,` or `}`".to_owned())
    );
    assert!(json::parse("").is_err());
    assert!(json::parse("{\"a\": \"unterminated}").is_err());
    assert!(json::parse("[1] 2").is_err());
    assert!(json::parse("{\"a\": 01x}").is_err());
    // Very deep nesting is refused, not a crash.
    let deep = "[".repeat(10_000);
    assert!(json::parse(&deep).is_err_and(|e| e.contains("deeper")));
}

#[test]
fn yaml_and_toml_keep_the_line_of_each_key() -> Result<(), String> {
    let yaml = "# comment\nserver:\n  host: example.org\n  timeout: 30\nlist:\n  - a\n  - b\n";
    let node = yaml::parse(yaml)?;
    assert_eq!(line_of(&node, "server"), Some(2));
    assert_eq!(line_of(&node, "list"), Some(5));
    let toml = "title = \"x\"\n\n[server]\ntimeout = 30\nhosts = [\"a\", \"b\"]\n\n[[bin]]\nname = \"one\"\n";
    let node = toml::parse(toml)?;
    assert_eq!(line_of(&node, "title"), Some(1));
    assert_eq!(line_of(&node, "server"), Some(3));
    assert!(yaml::parse("a: [1, 2\nb: 3").is_err_and(|e| e.starts_with("line ")));
    assert!(toml::parse("a = \n").is_err_and(|e| e.starts_with("line 1")));
    // Several YAML documents: a list of them. Types as YAML reads them.
    let Node::List(docs) = yaml::parse("a: 1\n---\nb: true\nc: ~\nd: 2024-01-02\ne: '1'\n")? else {
        return Err("not a list".to_owned());
    };
    assert_eq!(docs.len(), 2);
    let kinds: Vec<&str> = match &docs[1].node {
        Node::Map(f) => f.iter().map(|f| f.node.kind()).collect(),
        _ => Vec::new(),
    };
    assert_eq!(kinds, ["boolean", "null", "date", "string"]);
    Ok(())
}

#[test]
fn changed_values_show_as_key_paths_with_old_and_new() {
    let e = entries(
        "conf.json",
        "{\"server\": {\"timeout\": 30, \"host\": \"a\"}, \"debug\": false}",
        "{\"server\": {\"timeout\": 60, \"host\": \"a\", \"port\": 8080}, \"debug\": \"no\"}",
    );
    let line = |e: &Entry| {
        format!(
            "{:?} {} {} {}",
            e.change,
            e.path,
            e.old.clone().unwrap_or_default(),
            e.new.clone().unwrap_or_default()
        )
    };
    let shown: Vec<String> = e.iter().map(line).collect();
    assert_eq!(
        shown,
        [
            "Changed server.timeout 30 60",
            "Added server.port  8080",
            "Changed debug false \"no\"",
        ]
    );
    assert_eq!(e[2].kinds, Some(("boolean", "string")));
}

#[test]
fn reordered_keys_are_told_apart_from_changed_values() {
    let old = "a: 1\nb: 2\nc: 3\n";
    let new = "c: 3\na: 1\nb: 20\n";
    let e = entries("x.yaml", old, new);
    let summary: Vec<(Change, &str)> = e.iter().map(|e| (e.change, e.path.as_str())).collect();
    assert_eq!(summary, [(Change::Changed, "b"), (Change::Reordered, "c")]);
    let c = &e[1];
    assert_eq!((c.old_line, c.new_line), (Some(3), Some(1)));
    // Only reordered, and only formatting: nothing changed.
    assert!(entries("x.toml", "a = 1\nb = 2\n", "a   =   1 # same\nb = 2\n").is_empty());
}

#[test]
fn list_items_are_aligned_not_compared_by_index() {
    let old = "{\"items\": [{\"id\": 1}, {\"id\": 2}]}";
    let new = "{\"items\": [{\"id\": 0}, {\"id\": 1}, {\"id\": 2, \"x\": true}]}";
    let e = entries("a.json", old, new);
    let summary: Vec<(Change, &str)> = e.iter().map(|e| (e.change, e.path.as_str())).collect();
    assert_eq!(
        summary,
        [(Change::Added, "items[0]"), (Change::Added, "items[2].x")]
    );
}

#[test]
fn odd_keys_are_quoted_and_whole_files_added_or_removed() {
    let e = entries(
        "a.json",
        "{}",
        "{\"a.b\": 1, \"ключ\": 2, \"with space\": 3}",
    );
    let paths: Vec<&str> = e.iter().map(|e| e.path.as_str()).collect();
    assert_eq!(paths, ["\"a.b\"", "ключ", "\"with space\""]);
    let added = config::diff(
        "a.toml",
        &Sides {
            old: None,
            new: Some("a = 1\n".to_owned()),
        },
    );
    assert!(added.is_ok_and(|d| d.entries.len() == 1 && d.entries[0].change == Change::Added));
}

/// Malformed input falls back to the plain diff with a note naming the side and the line.
#[test]
fn malformed_config_falls_back_with_a_note() -> Result<(), String> {
    let err = config::diff("a.json", &sides("{\"a\": 1}", "{\"a\": 1,,}"));
    assert_eq!(
        err,
        Err(
            "the JSON after the change doesn't read: line 1: expected a key in quotes or `}`"
                .to_owned()
        )
    );
    Ok(())
}

#[test]
fn config_kinds_follow_the_extension() {
    assert_eq!(RichKind::of("Cargo.toml"), [RichKind::Config]);
    assert_eq!(RichKind::of("a/b c.YML"), [RichKind::Config]);
    assert_eq!(RichKind::of("tsconfig.jsonc"), [RichKind::Config]);
    assert!(RichKind::of("a.ini").is_empty());
}

/// Big files compare quickly.
#[test]
fn a_big_config_compares_quickly() {
    let make = |v: usize| -> String {
        let items: Vec<String> = (0..20_000)
            .map(|n| {
                format!(
                    "{{\"id\": {n}, \"value\": {}}}",
                    if n == 777 { v } else { n }
                )
            })
            .collect();
        format!("{{\"items\": [{}]}}", items.join(",\n"))
    };
    let started = std::time::Instant::now();
    let e = entries("big.json", &make(1), &make(2));
    assert!(started.elapsed() < std::time::Duration::from_secs(10));
    assert_eq!(e.len(), 1);
    assert_eq!(e[0].path, "items[777].value");
}

/// Read from a real repository through the same path the view takes.
#[test]
fn a_config_change_is_read_by_key_from_a_repository() -> Result<(), String> {
    use crate::git::{self, Area, StatusFile};
    let dir = std::env::temp_dir().join(format!("gitcraft-rich-config-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    for args in [
        &["init", "-q"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["config", "commit.gpgSign", "false"],
    ] {
        git::run(Some(&dir), args)?;
    }
    let name = "cönfig dir/app settings.yaml";
    let _ = std::fs::create_dir_all(dir.join("cönfig dir"));
    let _ = std::fs::write(dir.join(name), "server:\n  timeout: 30\n");
    git::run(Some(&dir), &["add", "-A"])?;
    git::run(Some(&dir), &["commit", "-qm", "one"])?;
    let _ = std::fs::write(dir.join(name), "server:\n  timeout: 60\n");
    let key = DiffKey::Work(StatusFile {
        path: name.to_owned(),
        old_path: None,
        code: 'M',
        area: Area::Unstaged,
    });
    let good = load(&dir, &key, RichKind::Config, None);
    let _ = std::fs::write(dir.join(name), "server:\n  timeout: [60\n");
    let bad = load(&dir, &key, RichKind::Config, None);
    let _ = std::fs::remove_dir_all(&dir);
    let Ok(Rich::Ready(RichDiff::Config(d))) = good else {
        return Err("not read by key".to_owned());
    };
    assert_eq!(d.entries.len(), 1);
    assert_eq!(d.entries[0].path, "server.timeout");
    assert_eq!(d.entries[0].new_line, Some(2));
    assert!(matches!(bad, Ok(Rich::Plain(note)) if note.contains("YAML after the change")));
    Ok(())
}

/// A row shows the path and both values, and a click goes to the key's line.
#[test]
fn the_config_view_goes_to_the_line_of_a_key() -> Result<(), String> {
    use egui_kittest::kittest::Queryable;
    let d = config::diff("a.yaml", &sides("a: 1\nb: 2\n", "a: 1\nb: 3\n"))?;
    let diff = RichDiff::Config(d);
    let acts = std::cell::RefCell::new(Vec::new());
    let state = std::cell::RefCell::new(RichState::default());
    let fonts = std::cell::Cell::new(false);
    let mut harness = egui_kittest::Harness::builder()
        .with_size([800.0, 400.0])
        .build_ui(|ui| {
            if !fonts.replace(true) {
                crate::theme::install_fonts(ui.ctx());
                return;
            }
            show(ui, &diff, &mut state.borrow_mut(), &mut acts.borrow_mut());
        });
    harness.run();
    harness.get_by_label_contains("b: 2 → 3").click();
    harness.run();
    assert!(acts.borrow().iter().any(|a| matches!(
        a,
        Action::DiffGoToLine {
            old: None,
            new: Some(2)
        }
    )));
    Ok(())
}
