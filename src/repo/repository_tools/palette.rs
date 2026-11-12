//! Command-palette contributions owned by this feature.
use crate::repo::Action;

use crate::repo::actions::PaletteContribution;

pub(in crate::repo) fn items() -> [PaletteContribution; 15] {
    [
        (
            "Conflicts that can wait: isolated rebase and restack…".into(),
            "Deferred rewrites".into(),
            Action::RepositoryTools(crate::repo::repository_tools::Act::Deferred),
        ),
        (
            "Parallel branches in this folder: hunk ownership…".into(),
            "Virtual branches".into(),
            Action::RepositoryTools(crate::repo::repository_tools::Act::Virtuals),
        ),
        (
            "Inspect state publication receipts…".into(),
            "Working-state transaction recovery".into(),
            Action::RepositoryTools(crate::repo::repository_tools::Act::StateReceipts),
        ),
        (
            "Named changelists: messages, commit and shelve…".into(),
            "Working changes".into(),
            Action::RepositoryTools(crate::repo::repository_tools::Act::Lists),
        ),
        (
            "Push outbox: queue exact commit or send selected entries…".into(),
            "Offline publication".into(),
            Action::RepositoryTools(crate::repo::repository_tools::Act::Outbox),
        ),
        (
            "Subtrees: register, update, inspect and publish vendored folders…".into(),
            "Vendored source history".into(),
            Action::RepositoryTools(crate::repo::repository_tools::Act::Subtrees(String::new())),
        ),
        (
            "Shared and borrowed objects".into(),
            "Inspect shared Git directories and donors; make a clone independent".into(),
            Action::RepositoryTools(crate::repo::repository_tools::Act::Sharing),
        ),
        (
            "Publish submodule dependencies before parent…".into(),
            "submodules publish remote".into(),
            Action::RepositoryTools(crate::repo::repository_tools::Act::Dependencies),
        ),
        (
            "Pull and push routes…".into(),
            "triangular upstream ahead behind".into(),
            Action::RepositoryTools(crate::repo::repository_tools::Act::Routes(String::new())),
        ),
        (
            "Publish to several hosts…".into(),
            "publication group retry mirrors".into(),
            Action::RepositoryTools(crate::repo::repository_tools::Act::Publication(
                String::new(),
            )),
        ),
        (
            "Remote URLs, refspecs and mirror policy…".into(),
            "fetch push mapping prune".into(),
            Action::RepositoryTools(crate::repo::repository_tools::Act::Remote(String::new())),
        ),
        (
            "Replace refs and temporary history joins…".into(),
            "grafts original effective history".into(),
            Action::RepositoryTools(crate::repo::repository_tools::Act::Replacements),
        ),
        (
            "Ref storage and reftable migration…".into(),
            "refs compatibility".into(),
            Action::RepositoryTools(crate::repo::repository_tools::Act::Reftable),
        ),
        (
            "How an object is stored…".into(),
            "objects pack delta file version".into(),
            Action::RepositoryTools(crate::repo::repository_tools::Act::Object("HEAD".into())),
        ),
        (
            "All refs and namespace history…".into(),
            "refs internals".into(),
            Action::RepositoryTools(crate::repo::repository_tools::Act::AllRefs(String::new())),
        ),
    ]
}
