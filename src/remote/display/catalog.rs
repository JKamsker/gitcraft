//! Finite exact-message catalogs, grouped by producer responsibility.
mod authority;
mod ledger;
mod native;
mod supervisor;
mod transport;

pub(super) fn entries() -> impl Iterator<Item = &'static (&'static str, &'static str, Option<u32>)>
{
    [
        transport::MESSAGES,
        native::MESSAGES,
        authority::MESSAGES,
        ledger::MESSAGES,
        supervisor::MESSAGES,
    ]
    .into_iter()
    .flatten()
}
