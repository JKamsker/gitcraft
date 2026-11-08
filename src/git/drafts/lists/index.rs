//! Changelists use the same narrow index transaction as repository_tools.
use super::*;
pub(super) struct Index(super::super::index_transaction::IndexTransaction);
impl Index {
    pub fn new(ctx: &Context, before: &Source, after: Vec<u8>) -> Result<Self, String> {
        if remote::is_remote(&ctx.repo) {
            return Err(crate::i18n::text(
                "backend-native-changelist-publication-is-required-remote-index-was-not-ch-8ac7c9",
            )
            .into());
        }
        let snapshot = before.cache_snapshot.as_deref().ok_or(crate::i18n::text(
            "backend-native-full-policy-index-snapshot-is-unavailable-no-index-changed-a78cc9",
        ))?;
        super::super::index_transaction::IndexTransaction::new(snapshot, after).map(Self)
    }
    pub fn evidence(&self) -> String {
        self.0.evidence()
    }
    pub fn accept(&mut self) -> Result<String, String> {
        self.0.accept()
    }
    pub fn abort(&mut self) -> Result<String, String> {
        self.0.abort()
    }
    pub fn uncertain(&mut self) -> String {
        self.0.retain()
    }
}
