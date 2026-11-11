//! Stable public palette labels, hints and typed targets across feature owners.
use super::*;
use crate::repo::{file_comparison, partial_stash, reflog, repository_tools, timeline};

fn target(action: &Action) -> &'static str {
    use file_comparison::Page;
    use repository_tools::Act;
    match action {
        Action::OpenFileComparison(Page::CodeMoves) => "moves",
        Action::OpenFileComparison(Page::Migration) => "migration",
        Action::OpenFileComparison(Page::Lens) => "staging",
        Action::OpenFileComparison(Page::Matrix) => "matrix",
        Action::RepositoryTools(Act::Deferred) => "deferred",
        Action::RepositoryTools(Act::Virtuals) => "virtuals",
        Action::RepositoryTools(Act::StateReceipts) => "receipts",
        Action::RepositoryTools(Act::Lists) => "lists",
        Action::RepositoryTools(Act::Outbox) => "outbox",
        Action::RepositoryTools(Act::Subtrees(folder)) if folder.is_empty() => "subtrees",
        Action::RepositoryTools(Act::Sharing) => "sharing",
        Action::RepositoryTools(Act::Dependencies) => "dependencies",
        Action::RepositoryTools(Act::Routes(remote)) if remote.is_empty() => "routes",
        Action::RepositoryTools(Act::Publication(group)) if group.is_empty() => "publication",
        Action::RepositoryTools(Act::Remote(remote)) if remote.is_empty() => "remote",
        Action::RepositoryTools(Act::Replacements) => "replacements",
        Action::RepositoryTools(Act::Reftable) => "reftable",
        Action::RepositoryTools(Act::Object(object)) if object == "HEAD" => "object",
        Action::RepositoryTools(Act::AllRefs(prefix)) if prefix.is_empty() => "refs",
        Action::Reflog(reflog::ReflogAct::Show(reflog::Source::Ref(name))) if name == "HEAD" => {
            "reflog"
        }
        Action::Reflog(reflog::ReflogAct::FindLost) => "lost",
        Action::Undo => "undo",
        Action::Redo => "redo",
        Action::ToggleTimeline => "timeline",
        Action::Dialog(Dialog::Stash(_)) => "stash",
        _ => "unexpected target",
    }
}

#[test]
fn feature_contributions_preserve_order_search_hints_and_exact_targets() {
    let entries = file_comparison::palette::items();
    let actual: Vec<_> = entries
        .iter()
        .map(|(name, hint, action)| (name.as_str(), hint.as_str(), target(action)))
        .collect();
    assert_eq!(
        actual,
        [
            ("Code moved across files", "refactor moved block", "moves"),
            ("File migration map", "files", "migration"),
            ("Three-state staging lens", "staging", "staging"),
            ("File versions across branches", "files", "matrix"),
        ]
    );
    let entries = repository_tools::palette::items();
    let actual: Vec<_> = entries
        .iter()
        .map(|(name, hint, action)| (name.as_str(), hint.as_str(), target(action)))
        .collect();
    assert_eq!(
        actual,
        [
            (
                "Conflicts that can wait: isolated rebase and restackâ€¦",
                "Deferred rewrites",
                "deferred"
            ),
            (
                "Parallel branches in this folder: hunk ownershipâ€¦",
                "Virtual branches",
                "virtuals"
            ),
            (
                "Inspect state publication receiptsâ€¦",
                "Working-state transaction recovery",
                "receipts"
            ),
            (
                "Named changelists: messages, commit and shelveâ€¦",
                "Working changes",
                "lists"
            ),
            (
                "Push outbox: queue exact commit or send selected entriesâ€¦",
                "Offline publication",
                "outbox"
            ),
            (
                "Subtrees: register, update, inspect and publish vendored foldersâ€¦",
                "Vendored source history",
                "subtrees"
            ),
            (
                "Shared and borrowed objects",
                "Inspect shared Git directories and donors; make a clone independent",
                "sharing"
            ),
            (
                "Publish submodule dependencies before parentâ€¦",
                "submodules publish remote",
                "dependencies"
            ),
            (
                "Pull and push routesâ€¦",
                "triangular upstream ahead behind",
                "routes"
            ),
            (
                "Publish to several hostsâ€¦",
                "publication group retry mirrors",
                "publication"
            ),
            (
                "Remote URLs, refspecs and mirror policyâ€¦",
                "fetch push mapping prune",
                "remote"
            ),
            (
                "Replace refs and temporary history joinsâ€¦",
                "grafts original effective history",
                "replacements"
            ),
            (
                "Ref storage and reftable migrationâ€¦",
                "refs compatibility",
                "reftable"
            ),
            (
                "How an object is storedâ€¦",
                "objects pack delta file version",
                "object"
            ),
            (
                "All refs and namespace historyâ€¦",
                "refs internals",
                "refs"
            ),
        ]
    );
    let entries = reflog::palette::items();
    let actual: Vec<_> = entries
        .iter()
        .map(|(name, hint, action)| (name.as_str(), hint.as_str(), target(action)))
        .collect();
    assert_eq!(
        actual,
        [
            ("Reflog", "recovery", "reflog"),
            ("Find lost commits", "recovery", "lost")
        ]
    );
    let entries = partial_stash::palette::snapshots();
    let actual: Vec<_> = entries
        .iter()
        .map(|(name, hint, action)| (name.as_str(), hint.as_str(), target(action)))
        .collect();
    assert_eq!(
        actual,
        [
            ("Stash only what is stagedâ€¦", "stash", "stash"),
            (
                "Save snapshot (stash, keep the changes)â€¦",
                "stash",
                "stash"
            ),
        ]
    );
    let ctx = egui::Context::default();
    let entries = timeline::palette::items(&ctx);
    assert_eq!(
        entries
            .iter()
            .map(|(_, _, action)| target(action))
            .collect::<Vec<_>>(),
        ["undo", "redo", "timeline"]
    );
    let entries = partial_stash::palette::stash(&ctx);
    assert_eq!(
        (&*entries[0].0, target(&entries[0].2)),
        ("Stash changesâ€¦", "stash")
    );
}
