"""Inert domain leaves for compiling production owner, cache and tab adapter."""
def source(directory):
    cache = (directory / 'deferred.rs').read_text()
    cache = cache[cache.index('#[derive(Default)]'):cache.index('pub(super) fn act')]
    return '''
    mod logic {
        pub struct View { pub jobs: Jobs }
        pub struct Jobs { pub jobs: Vec<Job> }
        pub struct Job { pub cancelled: bool, pub steps: Vec<Step> }
        pub struct Step {
            pub accepted: bool, pub old: String, pub branch: String, pub conflicts: Vec<String>
        }
    }
    mod deferred {
        use super::logic;
        use std::sync::atomic::Ordering;
    ''' + cache + '\n#[path = "' + str(directory / 'deferred/request_tests.rs') + '"] mod request_tests;\n' + '''
    }
    struct RepoTab {
        path: std::path::PathBuf, connection_context: crate::connections::Context,
        https_generation: u64, deferred: deferred::Cache, dialog: Option<Dialog>,
        outbox_reading: bool, outbox_last: Option<std::time::Instant>,
        busy_op: Option<String>, settled: usize,
    }
    // The pure feature fixture supplies inert central plumbing. Full application
    // context retirement and mutation delivery are covered in cochange_owner/tests.rs.
    impl RepoTab { fn notice_connections(&mut self, _: &egui::Context) {} }
    mod actions { pub(super) fn settlement(_: &mut super::RepoTab, _: super::owner::Settlement) {} }
    '''
