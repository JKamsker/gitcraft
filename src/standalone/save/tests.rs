use super::*;
fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    std::fs::write(path, bytes).map_err(|e| e.to_string())
}
fn read(path: &Path) -> Result<Vec<u8>, String> {
    std::fs::read(path).map_err(|e| e.to_string())
}

// Keep libtest names stable for the exact Windows shard filters.
include!("tests/reservation.rs");
include!("tests/transaction.rs");
include!("tests/unix_policy.rs");
include!("tests/mac_policy.rs");
include!("tests/windows_policy.rs");
