use super::*;

fn commit(hash: &str, parents: &[&str], hidden: bool) -> Arc<Commit> {
    Arc::new(Commit {
        hash: hash.into(),
        parents: parents.iter().map(|s| (*s).into()).collect(),
        author: if hidden { "T3 Code" } else { "Person" }.into(),
        email: if hidden {
            "t3code@users.noreply.github.com"
        } else {
            "human@example.com"
        }
        .into(),
        ..Default::default()
    })
}
#[test]
fn exact_default_identity_never_hides_a_human_cowriter_or_similar_account() {
    let rules = Rules::default();
    let base = commit("a", &[], true);
    assert!(rules.matches(&base));
    for (author, email) in [
        ("T3 Code", "human@example.com"),
        ("T3 Code helper", "t3code@users.noreply.github.com"),
        ("Human", "t3code@users.noreply.github.com"),
    ] {
        let c = Commit {
            author: author.into(),
            email: email.into(),
            body: "Co-authored-by: T3 Code <t3code@users.noreply.github.com>".into(),
            ..(*base).clone()
        };
        assert!(!rules.matches(&c));
    }
}
#[test]
fn consecutive_hidden_commits_and_hidden_merges_reconnect_every_parent_path() {
    let history = vec![
        commit("tip", &["h1"], false),
        commit("h1", &["merge"], true),
        commit("merge", &["h2", "side"], true),
        commit("h2", &["base"], true),
        commit("side", &["base"], false),
        commit("base", &[], false),
    ];
    let (visible, graph, hidden, kept) =
        compact(&history, &Rules::default(), &HashSet::new(), false, true);
    assert_eq!((hidden, kept), (3, 0));
    assert_eq!(
        visible.iter().map(|c| c.hash.as_str()).collect::<Vec<_>>(),
        ["tip", "side", "base"]
    );
    assert_eq!(graph[0].parents, ["base", "side"]);
    assert_eq!(visible[0].parents, ["h1"]); // Git operations retain original parents.
    assert_eq!(history[0].parents, ["h1"]);
    let layout = crate::graph::layout_with(&graph, false);
    assert_eq!(layout.rows.len(), 3);
    assert!(layout.width >= 2);
}
#[test]
fn tips_selection_search_stash_bases_and_pagination_frontiers_are_kept() {
    let mut tip = (*commit("tip", &["selected"], true)).clone();
    tip.refs.push("branch".into());
    let h = vec![
        Arc::new(tip),
        commit("selected", &["search"], true),
        commit("search", &["stash-base"], true),
        commit("stash-base", &["boundary"], true),
        commit("boundary", &["not-loaded-yet"], true),
    ];
    let protected = ["selected", "search", "stash-base"]
        .map(str::to_owned)
        .into_iter()
        .collect();
    let (visible, _, hidden, kept) = compact(&h, &Rules::default(), &protected, false, true);
    assert_eq!((visible.len(), hidden, kept), (5, 0, 5));
    let mut next = h.clone();
    next.push(commit("not-loaded-yet", &[], false));
    assert_eq!(
        compact(&next, &Rules::default(), &protected, false, true).2,
        1
    );
}
#[test]
fn global_additions_override_reveal_and_persistence_are_independent_per_repo() {
    let global = Rules {
        names: "Global".into(),
        ..Rules::default()
    };
    let mut p = Preferences {
        rules: Rules {
            names: "Local".into(),
            ..Rules::empty()
        },
        ..Default::default()
    };
    assert!(effective(&global, &p).names.contains("Global"));
    assert!(effective(&global, &p).names.contains("Local"));
    p.override_global = true;
    assert_eq!(effective(&global, &p), p.rules);
    p.reveal = true;
    assert_eq!(effective(&global, &p), Rules::empty());
    let ctx = egui::Context::default();
    let a = Path::new("/a");
    let b = Path::new("/b");
    ctx.data_mut(|d| d.insert_persisted(id(a), p.clone()));
    assert_eq!(load(&ctx, a), p);
    assert_eq!(load(&ctx, b), Preferences::default());
    let text = ron::to_string(&p).unwrap_or_default();
    assert_eq!(ron::from_str::<Preferences>(&text).ok(), Some(p));
}
#[test]
fn a_long_generated_chain_compacts_without_recursion() {
    let mut h = vec![commit("tip", &["0"], false)];
    for i in 0..10000 {
        h.push(commit(&i.to_string(), &[&(i + 1).to_string()], true));
    }
    h.push(commit("10000", &[], false));
    let (_, graph, hidden, _) = compact(&h, &Rules::default(), &HashSet::new(), false, true);
    assert_eq!(hidden, 10000);
    assert_eq!(graph[0].parents, ["10000"]);
}

#[test]
fn tab_navigation_and_search_reveal_hidden_rows_without_changing_operational_parents() {
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred("/nonexistent/history-navigation-test".into(), &ctx);
    tab.life.phase = crate::repo::lifecycle::Phase::Live;
    tab.history_filter.applied = Some(Rules::default());
    tab.refs.head = "tip".into();
    tab.history = vec![
        commit("tip", &["hidden"], false),
        commit("hidden", &["other"], true),
        commit("other", &["base"], true),
        commit("base", &[], false),
    ];
    tab.has_more = true;
    tab.rebuild_rows();
    assert_eq!(tab.commits.len(), 2);
    assert_eq!(tab.commits[0].parents, ["hidden"]);
    assert_eq!(tab.history.len(), 4); // pagination uses raw history, never compacted rows.
    tab.search.matches.insert("other".into());
    tab.goto("hidden", &ctx);
    assert_eq!(tab.selected_hash().as_deref(), Some("hidden"));
    assert!(tab.history_filter.revealed.is_empty());
    assert!(tab.index.contains_key("other"));
    assert_eq!(tab.commits[tab.index["hidden"]].parents, ["other"]);
    tab.rebuild(&ctx);
    assert_eq!(tab.selected_hash().as_deref(), Some("hidden"));
    assert!(tab.has_more);
}

#[test]
fn stale_namespace_search_results_cannot_restore_a_previous_scope() {
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred("/nonexistent/history-search-scope-test".into(), &ctx);
    tab.search.open = true;
    tab.search.query = "needle".into();
    tab.search.running = true;
    let _ = tab.tx.send(crate::repo::Msg::Search(
        Vec::new(),
        "needle".into(),
        false,
        Ok(vec!["old-checkpoint".into()]),
    ));
    tab.poll(&ctx);
    assert!(tab.search.results.is_empty());
    assert!(tab.search.running); // the current scope's request is still running.
    let _ = tab.tx.send(crate::repo::Msg::Search(
        rules(&ctx, tab.repo_key()).excludes(),
        "needle".into(),
        false,
        Ok(vec!["visible-commit".into()]),
    ));
    tab.poll(&ctx);
    assert_eq!(tab.search.results, ["visible-commit"]);
    assert!(!tab.search.running);
}

#[test]
fn a_selected_namespace_only_commit_bridges_both_old_parent_paths() {
    let selected = commit("selected", &["checkpoint"], true);
    let old = vec![
        selected.clone(),
        commit("checkpoint", &["main", "side"], true),
        commit("main", &["base"], false),
        commit("side", &["base"], false),
        commit("base", &[], false),
    ];
    let current = vec![old[2].clone(), old[3].clone(), old[4].clone()];
    let projected = pinned_projection(&selected, &old, &current);
    assert_eq!(projected.parents, ["main", "side"]);
    assert_eq!(selected.parents, ["checkpoint"]);
    // An ordinary reload can keep the projection after excluded raw ancestors are gone.
    assert_eq!(
        pinned_projection(&projected, &current, &current).parents,
        ["main", "side"]
    );
}

#[test]
fn hidden_parents_remain_in_every_operational_first_parent_line() {
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred("/nonexistent/filtered-operations".into(), &ctx);
    tab.life.phase = crate::repo::lifecycle::Phase::Live;
    tab.refs.head = "head".into();
    tab.refs.current = Some("main".into());
    tab.history_filter.applied = Some(Rules::default());
    tab.history = vec![
        commit("head", &["hidden"], false),
        commit("hidden", &["older"], true),
        commit("older", &["base"], false),
        commit("base", &[], false),
    ];
    tab.rebuild_rows();
    assert!(!tab.index.contains_key("hidden"));
    assert!(tab.history_index.contains_key("hidden"));
    let line =
        crate::repo::quick_edit::first_parents(&tab.history, &tab.history_index, &tab.refs.head);
    assert_eq!(
        line.iter().map(|c| c.hash.as_str()).collect::<Vec<_>>(),
        ["head", "hidden", "older", "base"]
    );
    for edit in [
        crate::repo::quick_edit::Edit::Drop,
        crate::repo::quick_edit::Edit::MoveUp,
    ] {
        assert_eq!(crate::repo::quick_edit::refusal(edit, &line, "older"), None);
    }
    assert_eq!(crate::repo::fold::plan::refusal(&line, "older"), None);
    assert_eq!(tab.surgery_refusal("older"), None);
    // Even a navigation history target hidden after it was recorded remains reachable at EOF.
    tab.has_more = false;
    tab.nav.visit(crate::repo::navigation::View::Graph {
        selected: Some("hidden".into()),
    });
    tab.nav.visit(crate::repo::navigation::View::Graph {
        selected: Some("head".into()),
    });
    tab.navigate(-1, &ctx);
    assert_eq!(tab.selected_hash().as_deref(), Some("hidden"));
}

#[test]
fn a_hidden_merge_still_blocks_flattening_rewrites() {
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred("/nonexistent/filtered-merge-safety".into(), &ctx);
    tab.refs.head = "head".into();
    tab.refs.current = Some("main".into());
    tab.history_filter.applied = Some(Rules::default());
    tab.history = vec![
        commit("head", &["merge"], false),
        commit("merge", &["older", "side"], true),
        commit("older", &["base"], false),
        commit("side", &["base"], false),
        commit("base", &[], false),
    ];
    tab.rebuild_rows();
    assert!(!tab.index.contains_key("merge"));
    assert!(tab.surgery_refusal("older").unwrap().contains("flatten"));
    let line = crate::repo::quick_edit::first_parents(&tab.history, &tab.history_index, "head");
    assert!(
        crate::repo::fold::plan::refusal(&line, "older")
            .unwrap()
            .contains("flatten")
    );
}

#[test]
fn first_parent_compaction_distinguishes_omitted_sides_from_unloaded_first_parents() {
    let history = vec![
        commit("head", &["merge"], false),
        commit("merge", &["base", "omitted-side"], true),
        commit("base", &[], false),
    ];
    for has_more in [false, true] {
        let (visible, graph, hidden, kept) =
            compact(&history, &Rules::default(), &HashSet::new(), true, has_more);
        assert_eq!((visible.len(), hidden, kept), (2, 1, 0));
        assert_eq!(graph[0].parents, ["base"]);
        assert_eq!(history[1].parents, ["base", "omitted-side"]);
    }
    let page = vec![
        commit("head", &["merge"], false),
        commit("merge", &["next-page", "omitted-side"], true),
    ];
    let (_, _, hidden, kept) = compact(&page, &Rules::default(), &HashSet::new(), true, true);
    assert_eq!((hidden, kept), (0, 1));
    let mut loaded = page;
    loaded.push(commit("next-page", &[], false));
    assert_eq!(
        compact(&loaded, &Rules::default(), &HashSet::new(), true, false).2,
        1
    );
    // Complete non-first-parent scope may also omit a parent due to Git history simplification.
    assert_eq!(
        compact(&history, &Rules::default(), &HashSet::new(), false, false).2,
        1
    );
}

#[test]
fn real_first_parent_git_log_can_compact_a_matching_merge() -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!(
        "gitcraft-filter-first-parent-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let git = |args: &[&str]| crate::git::run(Some(&dir), args);
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "Human"],
        &["config", "user.email", "human@example.com"],
        &["config", "commit.gpgSign", "false"],
        &["commit", "-q", "--allow-empty", "-m", "base"],
        &["branch", "side"],
        &["commit", "-q", "--allow-empty", "-m", "main work"],
        &["switch", "-q", "side"],
        &["commit", "-q", "--allow-empty", "-m", "side work"],
        &["switch", "-q", "main"],
        &["merge", "-q", "--no-ff", "side", "-m", "checkpoint merge"],
        &[
            "commit",
            "-q",
            "--amend",
            "--no-edit",
            "--author=T3 Code <t3code@users.noreply.github.com>",
        ],
        &["commit", "-q", "--allow-empty", "-m", "head"],
    ] {
        git(args)?;
    }
    let order = crate::git::LogOrder {
        first_parent: true,
        ..Default::default()
    };
    let roots = ["HEAD".to_owned()];
    let history: Vec<_> = crate::git::log(&dir, Some(&roots), order, 0, 50)?
        .into_iter()
        .map(Arc::new)
        .collect();
    let merge = history
        .iter()
        .find(|c| c.summary == "checkpoint merge")
        .unwrap();
    assert_eq!(merge.parents.len(), 2);
    assert!(!history.iter().any(|c| c.hash == merge.parents[1]));
    let (visible, graph, hidden, _) =
        compact(&history, &Rules::default(), &HashSet::new(), true, false);
    assert_eq!(hidden, 1);
    assert!(!visible.iter().any(|c| c.hash == merge.hash));
    assert_eq!(graph[0].parents, [merge.parents[0].clone()]);
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}
