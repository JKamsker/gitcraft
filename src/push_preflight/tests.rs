use super::*;
use std::path::PathBuf;

// Keep qualified test names stable for Windows test shards.
include!("tests/fixture.rs");
include!("tests/identity.rs");
include!("tests/ports.rs");
include!("tests/content.rs");
include!("tests/publication.rs");
include!("tests/repairs.rs");
include!("tests/lfs.rs");
include!("tests/routing.rs");
include!("tests/leases.rs");
