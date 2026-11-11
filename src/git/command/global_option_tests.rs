use super::{after_options, goes_over_network};
use crate::git::hooks::{subcommand, wants_trace};

#[test]
fn global_values_are_not_commands_for_execution_or_hooks() {
    for (args, expected, network, traced) in [
        (
            vec!["--git-dir", "/tmp/r.git", "fetch"],
            Some("fetch"),
            true,
            false,
        ),
        (vec!["--git-dir", "fetch", "push"], Some("push"), true, true),
        (
            vec!["--git-dir=push", "status"],
            Some("status"),
            false,
            false,
        ),
        (vec!["-Cpush", "commit"], Some("commit"), false, true),
        (vec!["-c", "push", "status"], Some("status"), false, false),
        (vec!["--git-dir"], None, false, false),
        (vec!["--future", "fetch", "push"], None, false, false),
        (vec!["\0try", "push"], None, false, false),
    ] {
        assert_eq!(subcommand(&args), expected, "{args:?}");
        assert_eq!(after_options(&args).first().copied(), expected, "{args:?}");
        assert_eq!(goes_over_network(&args), network, "{args:?}");
        assert_eq!(wants_trace(&args), traced, "{args:?}");
    }
}

#[test]
fn shared_syntax_does_not_merge_hook_and_execution_policies() {
    // Long-running execution classification intentionally includes local remote and
    // submodule commands, while hook tracing only includes commands that run hooks.
    for args in [["remote", "add"], ["submodule", "status"], ["lfs", "env"]] {
        assert!(goes_over_network(&args));
        assert!(!wants_trace(&args));
    }
    assert!(wants_trace(&["--work-tree", "push", "checkout"]));
    assert!(!goes_over_network(&["--work-tree", "push", "checkout"]));
}
