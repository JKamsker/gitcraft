//! Execution of a read effect, with the same guards and query semantics as before.
use super::*;
impl effects::Read {
    pub(super) fn run(self, path: &std::path::Path) -> Result<Data, String> {
        let Self { tool, query } = self;
        match tool {
            Tool::Subtrees => {
                crate::git::repository_tools::subtrees::read(path, &query).map(Data::Subtrees)
            }
            Tool::StateReceipts => crate::git::drafts::publication::read(path, &|| {
                !crate::tasks::cancellation().is_cancelled()
            })
            .map(Data::StateReceipts),
            Tool::Object => object::inspect(path, &query, &|| {
                !crate::tasks::cancellation().is_cancelled()
            })
            .map(Data::Object),
            Tool::Deferred => crate::git::drafts::deferred::read(path, &|| {
                !crate::tasks::cancellation().is_cancelled()
            })
            .map(Data::Deferred),
            Tool::Virtuals => crate::git::drafts::virtuals::read(path, &|| {
                !crate::tasks::cancellation().is_cancelled()
            })
            .map(Data::Virtual),
            Tool::Lists => crate::git::drafts::lists::read(path, &|| {
                !crate::tasks::cancellation().is_cancelled()
            })
            .map(Data::Lists),
            Tool::AggregateTurn => {
                let (endpoint, session) = query.split_once('\0').ok_or(crate::i18n::text(
                    "repo-workflows-repository_tools-repository-tools-act-38bb44e4",
                ))?;
                crate::git::drafts::turn::aggregate_review(path, session, endpoint, &|| {
                    !crate::tasks::cancellation().is_cancelled()
                })
                .map(Data::Turn)
            }
            Tool::Turn => {
                let parts = query.splitn(3, '\0').collect::<Vec<_>>();
                if parts.len() != 3 {
                    return Err(crate::i18n::text(
                        "repo-workflows-repository_tools-repository-tools-act-e7cf34fe",
                    )
                    .into());
                }
                crate::git::drafts::turn::review(path, parts[0], parts[1], parts[2], &|| {
                    !crate::tasks::cancellation().is_cancelled()
                })
                .map(Data::Turn)
            }
            Tool::Outbox => crate::git::repository_tools::outbox::read(path).map(Data::Outbox),
            Tool::Sharing => borrowing::inspect(path).map(Data::Sharing),
            Tool::Refs => all_refs::read(path, &query).map(Data::Refs),
            Tool::Reftable => reftable::inspect(path).map(Data::Reftable),
            Tool::Replacements => replacements::read(path).map(Data::Replacements),
            Tool::Remote => remote_config::inspect(path, &query).map(Data::Remote),
            Tool::Publication => publishing::read(path, &query).map(Data::Publication),
            Tool::Routes => routing::read(path, &query).map(Data::Routes),
            Tool::Dependencies => dependency::read(path).map(Data::Dependencies),
            Tool::SubmoduleConflict => subconflict::read(path, &query).map(Data::SubmoduleConflict),
            Tool::ForceRoute => {
                let (name, oid) = query.split_once('\0').ok_or(crate::i18n::text(
                    "repo-workflows-repository_tools-repository-tools-act-baa3959f",
                ))?;
                routing::prepare_force(path, name, oid).map(Data::Force)
            }
        }
    }
}
