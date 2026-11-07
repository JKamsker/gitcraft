use super::*;

pub(super) fn run(bench: &mut Bench, inputs: &Inputs) -> Result<(), String> {
    let repo = fixture()?;
    let path = repo.path();
    let parent = repo.rev("HEAD")?;
    repo.git(&["checkout", "-q", "-b", "feature-bench", "HEAD^"])?;
    let feature = repo.commit(
        "feature work",
        &[(
            "README.md",
            Some("# Improved project\n\nNew instructions.\n"),
        )],
    )?;
    repo.git(&["checkout", "-q", "main"])?;
    repo.git(&["checkout", "-q", "-b", "conflict-bench", "HEAD^"])?;
    let source =
        std::fs::read_to_string(repo.dir.join("src/main.rs")).map_err(|e| e.to_string())?;
    repo.commit(
        "conflicting work",
        &[("src/main.rs", Some(&source.replace("{ 100 }", "{ 102 }")))],
    )?;
    repo.git(&["checkout", "-q", "main"])?;
    let linked = repo.dir.join("build/linked");
    repo.git(&[
        "worktree",
        "add",
        "-q",
        "--detach",
        &linked.to_string_lossy(),
        "HEAD",
    ])?;
    bench.try_measure("repository/status-linked-worktree", || git::status(&linked));
    bench.try_measure("repository/status-clean-400-files", || git::status(path));
    bench.try_measure("repository/operation-state", || git::in_progress(path));
    bench.measure("repository/identity", || {
        black_box(git::identity(path));
    });
    bench.try_measure("repository/overview", || {
        git::overview(path, Duration::from_secs(10))
    });
    bench.try_measure("repository/refs", || git::refs(&inputs.repo));
    bench.try_measure("repository/worktrees", || git::worktree_list(path, Some(0)));
    bench.try_measure("repository/stashes", || git::stashes(path));
    bench.try_measure("repository/remotes", || git::remotes(path));
    bench.try_measure("repository/reload-metadata", || {
        std::thread::scope(|scope| {
            let refs = scope.spawn(|| git::refs(path));
            let status = scope.spawn(|| git::status(path));
            let history = scope.spawn(|| git::log(path, None, git::LogOrder::default(), 0, 1500));
            let stashes = scope.spawn(|| git::stashes(path));
            let identity = git::identity(path);
            let refs = refs.join().map_err(|e| format!("refs: {e:?}"))??;
            let status = status.join().map_err(|e| format!("status: {e:?}"))??;
            let history = history.join().map_err(|e| format!("history: {e:?}"))??;
            let stashes = stashes.join().map_err(|e| format!("stashes: {e:?}"))??;
            Ok((refs, status, history, stashes, identity))
        })
    });
    bench.try_measure("files/tree", || {
        git::tree_files(&inputs.repo, &inputs.revision)
    });
    bench.try_measure("files/read-blob", || {
        git::file_at(&inputs.repo, Some(&inputs.revision), "src/repo/load.rs")
    });
    bench.try_measure("files/history", || {
        git::file_history(&inputs.repo, Some(&inputs.revision), "src/repo/load.rs")
    });
    bench.try_measure("files/blame-2000-lines", || {
        git::blame_plain(path, Some("HEAD"), "src/main.rs", (1, 2000))
    });
    bench.try_measure("history/commit-detail", || {
        git::commit_detail(&inputs.repo, &inputs.revision)
    });
    bench.try_measure("history/next-page-500", || {
        git::log(
            &inputs.repo,
            Some(&inputs.revisions),
            git::LogOrder::default(),
            500,
            500,
        )
    });
    bench.try_measure("search/history-message", || {
        git::search(
            &inputs.repo,
            Some(&inputs.revisions),
            git::LogOrder::default(),
            "diff",
        )
    });
    let plan = git::search_query::prepare("author:Tëster path:src after:2020-01-01", false)
        .map_err(|e| format!("{e:?}"))?;
    bench.try_measure("search/history-operators", || {
        git::search_query::run(path, None, &plan)
    });
    // Code search uses a bounded project history; the large graph's full pickaxe history is a
    // different workload from interactive query parsing.
    bench.try_measure("search/code-history", || {
        git::search_code(path, None, git::LogOrder::default(), "item_100")
    });
    bench.try_measure("branches/by-date", || git::branches_by_date(&inputs.repo));
    bench.try_measure("branches/merged", || {
        git::merged_into(&inputs.repo, &inputs.revision)
    });
    bench.try_measure("metadata/notes-namespaces", || git::notes_namespaces(path));
    bench.try_measure("metadata/authorship", || {
        crate::authorship::load::one(&inputs.repo, &inputs.revision)
    });
    bench.try_measure("history/reflog", || git::reflog(path, "HEAD", 0, 200));
    let mcp = crate::mcp::tools::Ctx {
        repo: repo.dir.clone(),
        agent: "cpu-benchmark".into(),
        link_dir: None,
        wait: Duration::ZERO,
        cancelled: Arc::new(|| false),
    };
    for (name, args) in [
        ("status", serde_json::json!({})),
        ("branches", serde_json::json!({})),
        ("log", serde_json::json!({"limit":200})),
        ("show_commit", serde_json::json!({"commit":"HEAD"})),
        (
            "diff",
            serde_json::json!({"commit":"HEAD","max_bytes":40000}),
        ),
        (
            "blame",
            serde_json::json!({"path":"src/main.rs","rev":"HEAD","start_line":1,"end_line":200}),
        ),
    ] {
        let tool = crate::mcp::tools::find(name).ok_or("missing MCP read tool")?;
        let args = args.as_object().ok_or("MCP arguments")?;
        bench.try_measure(&format!("mcp/{name}-json-response"), || {
            let value = crate::mcp::tools::call(tool, &mcp, args)?;
            serde_json::to_vec(&value).map_err(|e| e.to_string())
        });
    }

    bench.try_measure("operations/checkout-cycle", || {
        repo.git(&["checkout", "-q", "feature-bench"])?;
        repo.git(&["checkout", "-q", "main"])
    });
    bench.try_measure("operations/merge-reset-cycle", || {
        repo.git(&["merge", "-q", "--no-ff", "--no-edit", "feature-bench"])?;
        repo.git(&["reset", "-q", "--hard", &parent])
    });
    bench.try_measure("operations/rebase-restore-cycle", || {
        repo.git(&["rebase", "main", "feature-bench"])?;
        repo.git(&["reset", "-q", "--hard", &feature])?;
        repo.git(&["checkout", "-q", "main"])
    });
    assert!(repo.git(&["merge", "--no-edit", "conflict-bench"]).is_err());
    assert!(matches!(
        git::in_progress(path)?,
        Some(git::InProgress::Merge)
    ));
    bench.try_measure("repository/status-merge-conflict", || git::status(path));
    let conflicts = crate::mcp::tools::find("conflicts").ok_or("missing conflicts tool")?;
    bench.try_measure("mcp/conflicts-json-response", || {
        let value = crate::mcp::tools::call(conflicts, &mcp, &serde_json::Map::new())?;
        serde_json::to_vec(&value).map_err(|e| e.to_string())
    });
    repo.git(&["merge", "--abort"])?;
    assert!(
        repo.git(&["-c", "rebase.backend=merge", "rebase", "conflict-bench"])
            .is_err()
    );
    assert!(matches!(
        git::in_progress(path)?,
        Some(git::InProgress::Rebase)
    ));
    bench.try_measure("repository/status-rebase-conflict", || git::status(path));
    repo.git(&["rebase", "--abort"])?;
    let remote = repo.dir.join("build/local.git");
    // Prepare an immutable bare source via Git's transport; local hardlink/copy setup can
    // race automatic object maintenance inherited from the host's configuration.
    repo.git(&[
        "clone",
        "-q",
        "--bare",
        "--no-local",
        ".",
        &remote.to_string_lossy(),
    ])?;
    bench.try_measure("transport/fetch-up-to-date-local", || {
        repo.git(&["fetch", "-q", &remote.to_string_lossy()])
    });
    bench.try_measure("transport/push-up-to-date-local", || {
        repo.git(&["push", "-q", &remote.to_string_lossy(), "main"])
    });
    let clone = repo.dir.join("build/clone");
    bench.try_measure("transport/clone-remove-local-cycle", || {
        git::clone(
            &remote.to_string_lossy(),
            &clone,
            None,
            &git::OpControl::new(|| {}),
        )?;
        std::fs::remove_dir_all(&clone).map_err(|e| e.to_string())
    });

    for i in 0..100 {
        write(
            &repo.dir.join(format!("src/package{}/file{i}.rs", i % 20)),
            format!(
                "// edited file {i}\npub fn value() -> usize {{ {} }}\n",
                i + 1
            ),
        )?;
        write(
            &repo.dir.join(format!("new/file{i}.txt")),
            format!("new file {i}\n"),
        )?;
    }
    repo.git(&["add", "src/package0", "src/package1"])?;
    let state = git::status(path)?;
    assert!(!state.staged.is_empty() && !state.unstaged.is_empty());
    bench.try_measure("repository/status-dirty-200-files", || git::status(path));
    let staged = state.staged.first().ok_or("no staged fixture file")?;
    let unstaged = state.unstaged.first().ok_or("no unstaged fixture file")?;
    for (name, file) in [("diff/index", staged), ("diff/worktree", unstaged)] {
        bench.try_measure(name, || {
            git::work_diff(path, file, git::DiffOptions::default())
        });
    }
    bench.try_measure("operations/stage-unstage-cycle", || {
        repo.git(&["add", "new"])?;
        repo.git(&["reset", "-q", "HEAD", "--", "new"])
    });
    bench.try_measure("operations/stash-restore-cycle", || {
        repo.git(&["stash", "push", "-q"])?;
        repo.git(&["stash", "pop", "-q", "--index"])
    });
    // Commit and restore the index: each timed iteration starts from the same parent/tree.
    let parent = repo.rev("HEAD")?;
    bench.try_measure("operations/commit-reset-cycle", || {
        repo.git(&["commit", "-q", "-m", "benchmark commit"])?;
        repo.git(&["reset", "-q", "--soft", &parent])
    });
    Ok(())
}
