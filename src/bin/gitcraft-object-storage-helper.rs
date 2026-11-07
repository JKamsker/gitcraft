//! Installed headless entry point for the versioned owned-object protocol.
fn main() {
    std::process::exit(gitcraft::app::object_storage_helper());
}
