//! Changelists use the same narrow index transaction as repository_tools.
use super::*;
pub(super) struct Index(super::super::index_transaction::IndexTransaction);
impl Index {
    pub fn new(ctx: &Context, before: &Source, after: Vec<u8>) -> Result<Self, String> {
        if remote::is_remote(&ctx.repo) {
            return Err(
                "Native changelist publication is required; remote index was not changed".into(),
            );
        }
        let snapshot = before
            .cache_snapshot
            .as_deref()
            .ok_or("Native full-policy index snapshot is unavailable; no index changed")?;
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
