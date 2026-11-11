use super::*;

fn leaf(name: &str, item: u32) -> Node<'_, u32> {
    Node::Leaf { name, item }
}

fn folder<'a>(name: &'a str, path: &'a str, children: Vec<Node<'a, u32>>) -> Node<'a, u32> {
    Node::Folder(Folder {
        name,
        path,
        children,
    })
}

fn only_folder<'a, 'b>(nodes: &'b [Node<'a, u32>]) -> Option<&'b Folder<'a, u32>> {
    match nodes {
        [Node::Folder(folder)] => Some(folder),
        _ => None,
    }
}

#[test]
fn names_without_a_slash_stay_flat() {
    assert_eq!(
        build([("main", 1), ("dev", 2)]),
        vec![leaf("main", 1), leaf("dev", 2)]
    );
}

#[test]
fn no_branches_make_no_nodes() {
    assert_eq!(build::<u32>([]), vec![]);
}

#[test]
fn a_shared_prefix_becomes_one_folder() {
    assert_eq!(
        build([("ci/coverage", 1), ("ci/hardening", 2)]),
        vec![folder(
            "ci",
            "ci",
            vec![leaf("coverage", 1), leaf("hardening", 2)]
        )]
    );
}

#[test]
fn folders_nest_and_know_their_whole_path() {
    assert_eq!(
        build([("docs/accuracy/gearbox", 1)]),
        vec![folder(
            "docs",
            "docs",
            vec![folder(
                "accuracy",
                "docs/accuracy",
                vec![leaf("gearbox", 1)]
            )]
        )]
    );
}

#[test]
fn siblings_keep_the_order_they_first_appear_in() {
    assert_eq!(
        build([("chore/a", 1), ("main", 2), ("t3/b", 3)]),
        vec![
            folder("chore", "chore", vec![leaf("a", 1)]),
            leaf("main", 2),
            folder("t3", "t3", vec![leaf("b", 3)]),
        ]
    );
}

#[test]
fn a_folder_collects_names_that_are_not_next_to_each_other() {
    assert_eq!(
        build([("ci/a", 1), ("main", 2), ("ci/b", 3)]),
        vec![
            folder("ci", "ci", vec![leaf("a", 1), leaf("b", 3)]),
            leaf("main", 2),
        ]
    );
}

#[test]
fn folders_of_the_same_name_in_different_places_stay_apart() {
    assert_eq!(
        build([("a/x/1", 1), ("b/x/2", 2)]),
        vec![
            folder("a", "a", vec![folder("x", "a/x", vec![leaf("1", 1)])]),
            folder("b", "b", vec![folder("x", "b/x", vec![leaf("2", 2)])]),
        ]
    );
}

#[test]
fn a_trailing_slash_names_a_leaf_not_an_empty_folder() {
    assert_eq!(
        build([("sub/", 1), ("lib/nested/", 2)]),
        vec![
            leaf("sub/", 1),
            folder("lib", "lib", vec![leaf("nested/", 2)]),
        ]
    );
}

#[test]
fn a_name_and_a_folder_of_the_same_name_stay_apart() {
    // A file replaced by a folder of the same name, in one commit.
    assert_eq!(
        build([("a", 1), ("a/b", 2)]),
        vec![leaf("a", 1), folder("a", "a", vec![leaf("b", 2)])]
    );
}

#[test]
fn unicode_names_split_at_their_slashes() {
    assert_eq!(
        build([("ünï/çødé/日本.rs", 1)]),
        vec![folder(
            "ünï",
            "ünï",
            vec![folder("çødé", "ünï/çødé", vec![leaf("日本.rs", 1)])]
        )]
    );
    assert_eq!(
        compacted(&[("ünï/çødé/日本.rs", 1)]),
        vec![folder("ünï/çødé", "ünï/çødé", vec![leaf("日本.rs", 1)])]
    );
}

#[test]
fn a_folder_counts_the_branches_at_any_depth() {
    let tree = build([("docs/a", 1), ("docs/deep/b", 2), ("docs/deep/c", 3)]);
    assert_eq!(only_folder(&tree).map(Folder::leaves), Some(3));
}

#[test]
fn a_folder_finds_a_branch_at_any_depth() {
    let tree = build([("docs/a", 1), ("docs/deep/b", 2)]);
    let has = |wanted: u32| only_folder(&tree).map(|f| f.any(&|item| *item == wanted));
    assert_eq!(has(2), Some(true));
    assert_eq!(has(9), Some(false));
}

fn compacted<'a>(items: &[(&'a str, u32)]) -> Vec<Node<'a, u32>> {
    let mut tree = build(items.iter().copied());
    compact(&mut tree);
    tree
}

#[test]
fn a_chain_of_lone_folders_becomes_one() {
    assert_eq!(
        compacted(&[("src/repo/mod.rs", 1), ("src/repo/diff.rs", 2)]),
        vec![folder(
            "src/repo",
            "src/repo",
            vec![leaf("mod.rs", 1), leaf("diff.rs", 2)]
        )]
    );
}

#[test]
fn a_folder_with_files_of_its_own_is_not_merged() {
    assert_eq!(
        compacted(&[("src/lib.rs", 1), ("src/repo/mod.rs", 2)]),
        vec![folder(
            "src",
            "src",
            vec![
                folder("repo", "src/repo", vec![leaf("mod.rs", 2)]),
                leaf("lib.rs", 1)
            ]
        )]
    );
}

#[test]
fn merging_stops_where_folders_branch() {
    assert_eq!(
        compacted(&[("a/b/c/1", 1), ("a/b/d/2", 2)]),
        vec![folder(
            "a/b",
            "a/b",
            vec![
                folder("c", "a/b/c", vec![leaf("1", 1)]),
                folder("d", "a/b/d", vec![leaf("2", 2)]),
            ]
        )]
    );
}

#[test]
fn folders_come_before_files_keeping_their_order() {
    assert_eq!(
        compacted(&[
            ("Cargo.toml", 1),
            ("src/a", 2),
            ("README.md", 3),
            ("docs/b", 4)
        ]),
        vec![
            folder("src", "src", vec![leaf("a", 2)]),
            folder("docs", "docs", vec![leaf("b", 4)]),
            leaf("Cargo.toml", 1),
            leaf("README.md", 3),
        ]
    );
}

#[test]
fn a_name_after_one_in_the_same_folders_goes_in_them() {
    assert_eq!(
        build([
            ("a/b/1", 1),
            ("a/c/2", 2),
            ("a/b/3", 3),
            ("a/c/", 4),
            ("a/c/d/5", 5)
        ]),
        vec![folder(
            "a",
            "a",
            vec![
                folder("b", "a/b", vec![leaf("1", 1), leaf("3", 3)]),
                folder(
                    "c",
                    "a/c",
                    vec![leaf("2", 2), folder("d", "a/c/d", vec![leaf("5", 5)])]
                ),
                leaf("c/", 4),
            ]
        )]
    );
}

/// A leaf out of sight whose menu is open is drawn all the same, found by its key (not its
/// place, which a reload can give another), or the menu would close.
#[test]
fn a_leaf_out_of_sight_with_its_menu_open_is_drawn() {
    let names: Vec<String> = (0..200).map(|i| format!("f{i}")).collect();
    let tree = build(names.iter().enumerate().map(|(i, n)| (n.as_str(), i)));
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let view = TreeView {
        id: Id::new("menu"),
        filtering: false,
        indent: 0.0,
        start_open: true,
        left: 18.0,
        leaf_h: Some(ROW_H),
    };
    let key = |i: &usize| ("leaf", *i);
    let mut drawn = Vec::new();
    for _ in 0..2 {
        drawn.clear();
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(300.0, 200.0))),
            ..Default::default()
        };
        ctx.run_ui(input, |ui| {
            ui.set_clip_rect(Rect::from_min_size(ui.cursor().min, vec2(300.0, 200.0)));
            let menu = ui.id().with(key(&150)).with("popup");
            if !egui::Popup::is_id_open(ui.ctx(), menu) {
                egui::Popup::open_id(ui.ctx(), menu);
            }
            show(ui, &tree, &view, &|_| false, &key, &mut |ui, _, i| {
                ui.allocate_space(vec2(10.0, ROW_H));
                drawn.push(*i);
            });
        })
        .drop_without_applying_deltas();
    }
    assert!(drawn.contains(&150), "{drawn:?}");
    assert!(!drawn.contains(&149), "{drawn:?}");
    assert!(drawn.contains(&0));
}

/// `a/f`, `a/a/f`, … `a/…/a/f` (`depth` folders deep), and a lone chain beside them.
fn deep_paths(depth: usize) -> Vec<String> {
    let mut paths: Vec<String> = (1..=depth).map(|d| "a/".repeat(d) + "f").collect();
    paths.push("b/".repeat(depth) + "f");
    paths
}

/// A repository can nest folders thousands deep: building the tree of a commit's files
/// (every frame), showing it and dropping it neither takes seconds nor runs out of stack.
#[test]
fn a_very_deep_tree_is_quick_and_needs_little_stack() -> Result<(), String> {
    const DEPTH: usize = 3000;
    let paths = deep_paths(DEPTH);
    let worker = std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(move || {
            let started = std::time::Instant::now();
            let mut tree = build(paths.iter().enumerate().map(|(i, p)| (p.as_str(), i)));
            compact(&mut tree);
            let built = started.elapsed();
            let count: usize = tree
                .iter()
                .map(|node| match node {
                    Node::Folder(folder) => folder.leaves(),
                    Node::Leaf { .. } => 1,
                })
                .sum();
            let deepest = tree.iter().any(|node| match node {
                Node::Folder(folder) => folder.any(&|i| *i == DEPTH - 1),
                Node::Leaf { .. } => false,
            });
            // The chain of lone folders is one row.
            let chain = tree.iter().find_map(|node| match node {
                Node::Folder(f) if f.name.starts_with('b') => Some(f.children.len()),
                _ => None,
            });
            // Every folder open, as file folders start.
            let ctx = egui::Context::default();
            crate::theme::install_fonts(&ctx);
            let mut rows = 0;
            ctx.run_ui(egui::RawInput::default(), |ui| {
                let view = TreeView {
                    id: Id::new("deep"),
                    filtering: false,
                    indent: 0.0,
                    start_open: true,
                    left: 18.0,
                    leaf_h: None,
                };
                show(ui, &tree, &view, &|_| false, &|i| *i, &mut |_, _, _| {
                    rows += 1
                });
            })
            .drop_without_applying_deltas();
            drop(tree);
            (built, count, deepest, chain, rows)
        })
        .map_err(|e| e.to_string())?;
    let (built, count, deepest, chain, rows) =
        worker.join().map_err(|_| "ran out of stack".to_owned())?;
    assert_eq!(count, DEPTH + 1);
    assert!(deepest);
    assert_eq!(chain, Some(1));
    assert_eq!(rows, DEPTH + 1);
    // It took about 5 s at 1500 deep, in a debug build.
    // A loose bound, so that a busy machine does not fail it: building it the old way
    // (time growing with the cube of the depth) would take tens of seconds here.
    assert!(built < std::time::Duration::from_secs(10), "{built:?}");
    Ok(())
}
