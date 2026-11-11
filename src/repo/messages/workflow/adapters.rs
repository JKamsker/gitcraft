use super::decimal;

#[derive(Clone, Copy)]
pub(in crate::repo::messages) enum Adapter {
    Operation,
    Scope,
    Health,
    Pick,
    Noun,
    Headline,
    Place,
    InProgress,
    Receipt,
}

pub(in crate::repo::messages) fn adapter(id: &str, name: &str) -> Option<Adapter> {
    match (id, name) {
        ("repo-workflows-operation-here-only", "operation")
        | ("repo-workflows-status-rewrite-retention", "operation") => Some(Adapter::Operation),
        ("repo-workflows-agent-agent-op-done-16d1e9b2", "label") => Some(Adapter::Operation),
        ("repo-workflows-agents-send-to-agent-4e9f0022", "label") => Some(Adapter::Operation),
        ("repo-workflows-bisect-bisect-step-done-8336fbfe", "label") => Some(Adapter::Operation),
        ("repo-workflows-detached-leave-to-28ae5898", "busy") => Some(Adapter::Operation),
        ("repo-workflows-detached-leave-to-007a4c0e", "busy") => Some(Adapter::Operation),
        ("repo-workflows-detached-detached-act-70dad4a3", "busy") => Some(Adapter::Operation),
        ("repo-workflows-doctor-set-row-921f564b", "scope-choice-scope-to-lowercase-3") => {
            Some(Adapter::Scope)
        }
        ("repo-workflows-doctor-turn-on-action-d8e3e02d", "s-title-to-lowercase-1") => {
            Some(Adapter::Health)
        }
        ("repo-workflows-repository_tools-repository-tool-mutation-988356ee", "busy") => {
            Some(Adapter::Operation)
        }
        ("repo-workflows-file_comparison-comparison-pick-28dc27ab", "pick-verb-1") => {
            Some(Adapter::Pick)
        }
        ("repo-workflows-hotfix-hotfix-op-done-fbdef87e", "run-label-1") => {
            Some(Adapter::Operation)
        }
        ("repo-workflows-issue_work-start-issue-e2b80c1b", "busy") => Some(Adapter::Operation),
        ("repo-workflows-issue_work-refresh-issue-a0e9ad2a", "label") => Some(Adapter::Operation),
        ("repo-workflows-issue_work-refresh-issue-a0e9ad2a", "busy") => Some(Adapter::Operation),
        ("repo-workflows-issue_work-refresh-issue-8336fbfe", "label") => Some(Adapter::Operation),
        ("repo-workflows-lfs_locks-lock-job-35b33376", "label") => Some(Adapter::Operation),
        ("repo-workflows-lfs_locks-lock-job-36f5baee", "label") => Some(Adapter::Operation),
        ("repo-workflows-lfs_locks-lock-job-8336fbfe", "label") => Some(Adapter::Operation),
        ("repo-workflows-partial_stash-stash-8336fbfe", "label") => Some(Adapter::Operation),
        ("repo-workflows-push-push-op-ended-79357f2e", "label") => Some(Adapter::Operation),
        ("repo-workflows-push-chain-step-ended-dfa32698", "what") => Some(Adapter::Operation),
        ("repo-workflows-push-remote-checked-3bbec358", "banner-headline-r-1") => {
            Some(Adapter::Headline)
        }
        ("repo-workflows-release-release-stopped-a4afd9be", "label") => Some(Adapter::Operation),
        ("repo-workflows-release-release-stopped-a4afd9be", "step-label-1") => {
            Some(Adapter::Operation)
        }
        ("repo-workflows-release-release-stopped-328c7ab5", "label") => Some(Adapter::Operation),
        ("repo-workflows-ssh_setup-launch-2a66f8c5", "step-label-1") => Some(Adapter::Operation),
        ("repo-workflows-ssh_setup-poll-b3badea2", "run-step-label-1") => Some(Adapter::Operation),
        ("repo-workflows-ssh_setup-poll-b5bb4cca", "run-step-label-1") => Some(Adapter::Operation),
        ("repo-workflows-stack-run-stack-op-95c7214f", "label") => Some(Adapter::Operation),
        ("repo-workflows-stack-run-stack-op-95c7214f", "busy") => Some(Adapter::Operation),
        ("repo-workflows-stack-start-publish-e0c26cc9", "busy") => Some(Adapter::Operation),
        ("repo-workflows-stack-start-publish-c4ffa0c1", "noun") => Some(Adapter::Noun),
        ("repo-workflows-stack-publish-done-c4ffa0c1", "noun") => Some(Adapter::Noun),
        ("repo-workflows-stack-publish-done-6ffb5e8c", "noun") => Some(Adapter::Noun),
        ("repo-workflows-stack-summary-47eeff3f", "noun") => Some(Adapter::Noun),
        ("repo-workflows-stack-summary-7f75368f", "noun") => Some(Adapter::Noun),
        ("repo-workflows-stack-run-restack-95c7214f", "label") => Some(Adapter::Operation),
        ("repo-workflows-stack-run-restack-95c7214f", "busy") => Some(Adapter::Operation),
        ("repo-workflows-stack-restack-done-33b4b547", "place") => Some(Adapter::Place),
        ("repo-workflows-surgery-open-absorb-edcc444b", "state-label-to-lowercase-1") => {
            Some(Adapter::InProgress)
        }
        ("repo-workflows-timeline-outside-label-d1d10f56", "first") => Some(Adapter::Operation),
        ("repo-workflows-timeline-fallback-label-4da74cbd", "one-c-1") => Some(Adapter::Operation),
        ("repo-workflows-timeline-discard-remove-36f5baee", "label") => Some(Adapter::Operation),
        ("repo-workflows-timeline-discard-remove-8336fbfe", "label") => Some(Adapter::Operation),
        ("repo-workflows-timeline-timeline-op-ended-be0297e1", "label") => Some(Adapter::Operation),
        ("repo-workflows-timeline-show-receipt-e172bf9c", "label") => Some(Adapter::Operation),
        ("repo-workflows-timeline-show-receipt-e172bf9c", "receipt-sentence-1") => {
            Some(Adapter::Receipt)
        }
        ("repo-workflows-timeline-undo-bf959af0", "entry-label-1") => Some(Adapter::Operation),
        ("repo-workflows-timeline-redo-2b66f1cb", "entry-label-1") => Some(Adapter::Operation),
        ("repo-workflows-timeline-jump-b77dfff9", "label") => Some(Adapter::Operation),
        ("repo-workflows-timeline-timeline-unsettled-21705388", "what") => Some(Adapter::Operation),
        ("repo-workflows-timeline-timeline-unsettled-3fd5b04c", "what") => Some(Adapter::Operation),
        _ => None,
    }
}

pub(in crate::repo::messages) fn scope(raw: &str) -> Option<String> {
    let id = match raw {
        "this repository" => "repo-workflows-doctor-scope-choice-38429e22",
        "this worktree" => "repo-workflows-doctor-scope-choice-0eb26325",
        "all my repositories" => "repo-workflows-doctor-scope-choice-59b8031f",
        "everyone on this machine" => "repo-workflows-doctor-scope-choice-ad944dcb",
        "command line" => "repo-workflows-doctor-scope-choice-c925a118",
        _ => return None,
    };
    Some(crate::i18n::text(id).to_owned())
}
pub(in crate::repo::messages) fn health(raw: &str) -> Option<String> {
    let id = match raw {
        "commit-graph" => "backend-health-commit-graph",
        "scheduled maintenance" => "backend-scheduled-maintenance-f688c9",
        "file system monitor" => "backend-file-system-monitor-bd6322",
        "untracked cache" => "backend-untracked-cache-de0942",
        "pack the objects" => "backend-pack-the-objects-5a853d",
        _ => return None,
    };
    Some(crate::i18n::text(id).to_owned())
}
pub(in crate::repo::messages) fn noun(raw: &str) -> Option<String> {
    match raw {
        "pull request" => Some(crate::forge::Forge::GitHub.noun_label().to_owned()),
        "merge request" => Some(crate::forge::Forge::GitLab.noun_label().to_owned()),
        _ => None,
    }
}
pub(in crate::repo::messages) fn place(raw: &str) -> Option<String> {
    if raw.is_empty() {
        return Some(String::new());
    }
    let path = raw.strip_prefix(" in its worktree at ")?;
    Some(crate::i18n::message(
        "repo-workflows-stack-restack-done-6cecc855",
        &[("wt-display-1", path.into())],
    ))
}
pub(in crate::repo::messages) fn in_progress(raw: &str) -> Option<String> {
    let state = match raw {
        "merge" => crate::git::InProgress::Merge,
        "cherry-pick" => crate::git::InProgress::CherryPick,
        "revert" => crate::git::InProgress::Revert,
        "rebase" => crate::git::InProgress::Rebase,
        "patch" => crate::git::InProgress::Am,
        _ => return None,
    };
    Some(state.label().to_owned())
}

pub(in crate::repo::messages) fn headline(raw: &str) -> Option<String> {
    let (remote, count, clear) =
        if let Some(remote) = raw.strip_suffix(" has nothing you don't have now: push again") {
            (remote, 0, "yes")
        } else {
            let body = raw.strip_suffix(" you don't have")?;
            let (remote, rest) = body.rsplit_once(" has ")?;
            let (number, plural) = rest.split_once(" commit")?;
            let count = decimal(number)?;
            if plural != if count == 1 { "" } else { "s" } {
                return None;
            }
            (remote, count, "no")
        };
    Some(crate::i18n::message(
        "repo-workflows-push-headline",
        &[
            ("remote", remote.into()),
            ("count", count.into()),
            ("clear", clear.into()),
        ],
    ))
}
