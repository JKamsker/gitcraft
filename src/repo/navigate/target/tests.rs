//! Destination ownership checks for editors whose open handlers may touch Git.
use super::*;
use crate::{
    git,
    repo::{DiffKey, file_comparison::Page, file_view::FileTab, review_workflow::ReviewSource},
};

fn destinations() -> Vec<Action> {
    let hash = "a".repeat(40);
    let key = DiffKey::Commit {
        hash: hash.clone(),
        path: "file.txt".into(),
    };
    let mut actions = vec![
        Action::Select(0),
        Action::SelectHash(hash.clone()),
        Action::SelectBranch(hash.clone()),
        Action::SelectStash(0),
        Action::SelectPr(1),
        Action::FocusCommit,
        Action::ActivityCommit(hash.clone()),
        Action::OpenCommitChanges(hash.clone()),
        Action::OpenDiff(key),
        Action::FileView(crate::repo::file_view::FileViewAct::DiffOfLines {
            hash: hash.clone(),
            path: "file.txt".into(),
            lines: vec![(1, 2)],
        }),
        Action::CloseDiff,
        Action::FileView(crate::repo::file_view::FileViewAct::Open {
            path: "file.txt".into(),
            rev: None,
            tab: FileTab::History,
        }),
        Action::FileView(crate::repo::file_view::FileViewAct::LineHistory {
            path: "file.txt".into(),
            at: git::LinesAt::Rev(hash.clone()),
            range: (1, 2),
        }),
        Action::FileView(crate::repo::file_view::FileViewAct::BlameAt {
            path: "file.txt".into(),
            rev: None,
            line: 1,
        }),
        Action::OpenPinnedFileLine(git::investigations::dossier::Seed {
            commit: hash.clone(),
            path: "file.txt".into(),
            line: 1,
        }),
        Action::OpenLineDossier(None),
        Action::OpenSymbolBiography(None),
        Action::OpenPr(1),
        Action::OpenCi {
            sha: hash.clone(),
            run: None,
        },
        Action::OpenTour,
        Action::Compare {
            base: hash.clone(),
            base_label: "base".into(),
            target: None,
            target_label: "work".into(),
            captions: [None, None],
            since_fork: false,
        },
        Action::Story(StoryAct::OpenCode {
            commit: hash.clone(),
            path: "file.txt".into(),
            line: 1,
        }),
        Action::Surgery(crate::repo::surgery::SurgeryAct::OpenSplit(hash.clone())),
        Action::Surgery(crate::repo::surgery::SurgeryAct::OpenMove(hash.clone())),
        Action::Surgery(crate::repo::surgery::SurgeryAct::OpenAbsorb),
        Action::Rewrite(crate::repo::rewrite::RewriteAct::InteractiveRebase {
            onto: hash.clone(),
            label: "base".into(),
        }),
        Action::Versions(VersionsAct::Rewrite(git::Rewrite {
            branch: "main".into(),
            time: 1,
            old: git::Series::new(&hash, &hash),
            new: git::Series::new(&hash, &hash),
        })),
        Action::Versions(VersionsAct::Branches {
            old: "main".into(),
            old_label: "old".into(),
            new: "feature".into(),
            new_label: "new".into(),
        }),
        Action::Versions(VersionsAct::PrSince(1)),
        Action::Bake(BakeAct::Compare(Vec::new())),
        Action::Resolver(ResolverAct::Review(0)),
        Action::ToggleTimeline,
        Action::Reflog(ReflogAct::Show(crate::repo::reflog::Source::head())),
        Action::Reflog(ReflogAct::FindLost),
        Action::Reflog(ReflogAct::Select(0)),
        Action::Comparison(crate::repo::file_comparison::ComparisonAct::NormalPatch),
        Action::Navigate(-1),
    ];
    actions.extend(
        crate::repo::insights::Kind::ALL.map(|kind| Action::Insights(InsightsAct::Open(kind))),
    );
    actions.extend(crate::repo::doctor::Section::ALL.map(Action::OpenDoctor));
    actions.extend(
        [Page::Migration, Page::CodeMoves, Page::Lens, Page::Matrix]
            .map(|page| Action::Comparison(crate::repo::file_comparison::ComparisonAct::Open(page))),
    );
    actions.extend(
        [
            ReviewSource::Working,
            ReviewSource::Commit(hash.clone()),
            ReviewSource::Comparison {
                from: hash,
                to: None,
            },
            ReviewSource::PullRequest {
                number: 1,
                url: "https://example.invalid/pull/1".into(),
            },
        ]
        .map(|source| Action::Review(ReviewAct::Open(source))),
    );
    actions
}

#[test]
fn all_primary_destination_intents_retire_the_older_restoration() {
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred("/recording-only/destination-classification".into(), &ctx);
    for (index, action) in destinations().into_iter().enumerate() {
        tab.pending_goto = Some(PendingGoto::Restore("old".into()));
        tab.retire_navigation_restore(&action);
        assert!(tab.pending_goto.is_none(), "destination {index}");
        // A row, pill, stash or pull request picked directly also retires a paging link;
        // any other destination leaves it to arrive.
        let direct = matches!(
            action,
            Action::Select(_)
                | Action::SelectBranchTip(_)
                | Action::SelectStash(_)
                | Action::SelectPr(_)
        );
        for link in [
            PendingGoto::History("new".into()),
            PendingGoto::BranchTip("new".into()),
        ] {
            tab.pending_goto = Some(link);
            tab.retire_navigation_restore(&action);
            assert_eq!(
                tab.pending_goto.is_none(),
                direct,
                "destination {index} kept or dropped the paging link wrongly"
            );
        }
    }
}

#[test]
fn controls_within_the_same_destination_preserve_the_pending_selection() {
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred("/recording-only/destination-controls".into(), &ctx);
    for action in [
        Action::Diff(crate::repo::diff::DiffAct::BesideGraph(true)),
        Action::Insights(InsightsAct::ToggleTable),
        Action::Review(ReviewAct::Reload),
        Action::Versions(VersionsAct::Select(0)),
        Action::Reflog(ReflogAct::Reload),
        Action::Bake(BakeAct::OpenFile("file.txt".into())),
        Action::Resolver(ResolverAct::ReviewAsDiff(true)),
        Action::Comparison(crate::repo::file_comparison::ComparisonAct::Refresh),
    ] {
        tab.pending_goto = Some(PendingGoto::Restore("target".into()));
        tab.retire_navigation_restore(&action);
        assert!(
            tab.pending_goto
                .as_ref()
                .is_some_and(|target| target.restoring() && target.hash() == "target")
        );
    }
}
