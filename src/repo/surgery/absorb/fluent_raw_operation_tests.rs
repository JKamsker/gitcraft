use super::*;
#[test]
fn fluent_partition_absorb_raw_in_progress_identity_and_command_are_locale_independent() {
    for language in ["en-US", "de", "en-US"] {
        crate::i18n::set_language(language);
        for (state, raw, command) in [
            (git::InProgress::Merge, "merge", "merge"),
            (git::InProgress::CherryPick, "cherry-pick", "cherry-pick"),
            (git::InProgress::Revert, "revert", "revert"),
            (git::InProgress::Rebase, "rebase", "rebase"),
            (git::InProgress::Am, "patch", "am"),
        ] {
            assert_eq!(
                in_progress_status(state),
                format!("Distribute staged fixes: a {raw} is in progress")
            );
            assert_eq!(state.command(), command);
        }
    }
}
