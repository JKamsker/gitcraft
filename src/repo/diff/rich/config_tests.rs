//! Tests of config files by key: the readers (lines, errors, odd input) and the comparison.

use super::config::{self, Change, Entry};
use super::tree::{Node, Ty};
use super::*;

#[test]
fn bulk_removals_keep_source_order_and_duplicate_anchors() -> Result<(), String> {
    let old = r#"{"gone":0,"anchor":0,"first":1,"second":2,"anchor":3,"last":4,"dup":5,"dup":6}"#;
    let diff = config::diff("config.json", &sides(old, r#"{"anchor":3}"#))?;
    let paths: Vec<_> = diff.entries.iter().map(|e| e.path.as_str()).collect();
    assert_eq!(paths, ["gone", "first", "second", "last", "dup"]);
    assert!(diff.entries.iter().all(|e| e.change == Change::Removed));
    assert_eq!(
        diff.entries.last().and_then(|e| e.old.as_deref()),
        Some("6")
    );
    let old = format!(
        "{{{}}}",
        (0..5000)
            .map(|i| format!("\"key{i}\":{i}"))
            .collect::<Vec<_>>()
            .join(",")
    );
    let diff = config::diff("config.json", &sides(&old, r#"{"key0":0}"#))?;
    assert_eq!(diff.entries.len(), 4999);
    assert_eq!(diff.entries[0].path, "key1");
    assert_eq!(diff.entries[4998].path, "key4999");
    Ok(())
}

#[test]
fn whole_config_removal_and_missing_sides_are_distinguished() -> Result<(), String> {
    let removed = config::diff(
        "settings.json",
        &Sides {
            old: Some("{\"enabled\":true}".into()),
            new: None,
        },
    )?;
    assert_eq!(removed.entries.len(), 1);
    assert_eq!(removed.entries[0].change, Change::Removed);
    assert_eq!(removed.entries[0].path, "(the whole file)");
    assert!(
        config::diff(
            "settings.json",
            &Sides {
                old: None,
                new: None
            }
        )?
        .entries
        .is_empty()
    );
    assert!(config::diff("unsupported.xml", &sides("<old/>", "<new/>")).is_err());
    let error = config::diff("settings.json", &sides("{broken", "{}"));
    assert!(error.is_err_and(|e| e.contains("before")));
    Ok(())
}

#[test]
fn large_config_rewrites_keep_value_changes_when_alignment_is_bounded() -> Result<(), String> {
    let n = 2001;
    let old = format!(
        "[{}]",
        (0..n).map(|i| i.to_string()).collect::<Vec<_>>().join(",")
    );
    let new = format!(
        "[{}]",
        (n..2 * n)
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(",")
    );
    let diff = config::diff("settings.json", &sides(&old, &new))?;
    assert_eq!(diff.entries.len(), n);
    assert_eq!(diff.entries[0].path, "[0]");
    assert_eq!(diff.entries[n - 1].path, format!("[{}]", n - 1));
    assert!(diff.entries.iter().all(|e| e.change == Change::Changed));
    let old = format!(
        "{{{}}}",
        (0..n)
            .map(|i| format!("\"key{i}\":{i}"))
            .collect::<Vec<_>>()
            .join(",")
    );
    let new = format!(
        "{{{}}}",
        (0..n)
            .rev()
            .map(|i| format!("\"key{i}\":{}", i + usize::from(i == 0)))
            .collect::<Vec<_>>()
            .join(",")
    );
    let diff = config::diff("settings.json", &sides(&old, &new))?;
    assert_eq!(diff.entries.len(), 1);
    assert_eq!(diff.entries[0].path, "key0");
    assert_eq!(diff.entries[0].change, Change::Changed);
    Ok(())
}

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
            "the JSON after the change isn't valid (line 1: expected a key in quotes or `}`)"
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

/// TOML numbers as written: `1` → `1.0` is a change; hexadecimal stays so.
#[test]
fn toml_numbers_keep_how_they_are_written() {
    let e = entries("a.toml", "x = 1\ny = 0x10\n", "x = 1.0\ny = 0x10\n");
    assert_eq!(e.len(), 1);
    assert_eq!(
        (e[0].old.as_deref(), e[0].new.as_deref()),
        (Some("1"), Some("1.0"))
    );
}

/// An alias is what its anchor holds: renaming anchors changes nothing, changing what the
/// anchor holds shows where it is used.
#[test]
fn yaml_aliases_compare_by_what_they_repeat() {
    let old = "base: &a {x: 1}\nuse: *a\n";
    let renamed = "extra: &z 5\nbase: &b {x: 1}\nuse: *b\n";
    let e = entries("a.yaml", old, renamed);
    let paths: Vec<&str> = e.iter().map(|e| e.path.as_str()).collect();
    assert_eq!(paths, ["extra"]);
    let changed = entries("a.yaml", old, "base: &a {x: 2}\nuse: *a\n");
    let paths: Vec<&str> = changed.iter().map(|e| e.path.as_str()).collect();
    assert_eq!(paths, ["base.x", "use.x"]);
    // An alias bomb is cut short, not followed for ever.
    let mut bomb = String::from("a0: &a0 [x, x, x, x, x, x, x, x, x, x]\n");
    for k in 1..12 {
        let refs = vec![format!("*a{}", k - 1); 10].join(", ");
        bomb.push_str(&format!("a{k}: &a{k} [{refs}]\n"));
    }
    let started = std::time::Instant::now();
    assert!(yaml::parse(&bomb).is_ok());
    assert!(started.elapsed() < std::time::Duration::from_secs(10));
}

/// Maps with tens of thousands of keys, and one item inserted in a long list.
#[test]
fn big_maps_and_lists_compare_quickly_and_align() {
    let map = |v: usize| -> String {
        let keys: Vec<String> = (0..50_000)
            .map(|n| format!("k{n} = {}", if n == 9 { v } else { n }))
            .collect();
        keys.join("\n")
    };
    let started = std::time::Instant::now();
    let e = entries("big.toml", &map(1), &map(2));
    assert!(
        started.elapsed() < std::time::Duration::from_secs(10),
        "{:?}",
        started.elapsed()
    );
    assert_eq!(e.len(), 1);
    let list = |extra: bool| -> String {
        let mut items: Vec<String> = (0..3000).map(|n| n.to_string()).collect();
        if extra {
            items.insert(1500, "\"new\"".to_owned());
        }
        format!("[{}]", items.join(","))
    };
    let e = entries("l.json", &list(false), &list(true));
    assert_eq!(e.len(), 1);
    assert_eq!((e[0].change, e[0].path.as_str()), (Change::Added, "[1500]"));
}

/// A key written twice counts as its last value, as parsers keep it.
#[test]
fn a_key_written_twice_is_its_last_value() {
    let e = entries("a.json", "{\"a\": 1, \"a\": 2}", "{\"a\": 1}");
    assert_eq!(e.len(), 1);
    assert_eq!(
        (e[0].old.as_deref(), e[0].new.as_deref()),
        (Some("2"), Some("1"))
    );
    assert!(entries("a.json", "{\"a\": 1, \"a\": 1}", "{\"a\": 1, \"a\": 2}").len() == 1);
}

#[test]
fn json_is_strict_where_it_should_be() -> Result<(), String> {
    // A lone half of a surrogate pair doesn't swallow what follows.
    let Node::Scalar { text, .. } = json::parse("\"\\ud800A\"")? else {
        return Err("not a string".to_owned());
    };
    assert_eq!(text, "\u{fffd}A");
    for bad in ["01", "1.", "-.5", "1.e3", "\"a\tb\""] {
        assert!(json::parse(bad).is_err(), "{bad}");
    }
    for good in ["0", "-0.5", "1e300", "1E-3", "10"] {
        assert!(json::parse(good).is_ok(), "{good}");
    }
    Ok(())
}

#[test]
fn yaml_types_follow_its_core_schema() -> Result<(), String> {
    let Node::Map(f) = yaml::parse(
        "a: inf\nb: .inf\nc: nan\nd: !!str 1\ne: 2024-01-01xyz\nf: 2024-01-01T10:00:00Z\ng: 0x1F\n",
    )?
    else {
        return Err("not a map".to_owned());
    };
    let kinds: Vec<&str> = f.iter().map(|f| f.node.kind()).collect();
    assert_eq!(
        kinds,
        [
            "string", "number", "string", "string", "string", "date", "number"
        ]
    );
    // Keys that are lists are told apart by what they hold.
    let e = entries(
        "a.yaml",
        "? [a, b]\n: 1\n? [c, d]\n: 2\n",
        "? [a, b]\n: 1\n? [c, e]\n: 3\n",
    );
    let paths: Vec<&str> = e.iter().map(|e| e.path.as_str()).collect();
    assert_eq!(paths, ["\"[c, e]\"", "\"[c, d]\""]);
    Ok(())
}
