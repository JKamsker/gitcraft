use super::*;
#[test]
fn command_values_and_invalid_enum_values_cannot_enter_an_issue_query() {
    let all = config::parse_origins(
        "local\0file:/owned/config\0core.editor\nenv API_TOKEN=FIXTURE_SECRET nvim\0local\0file:/owned/config\0pull.rebase\nFIXTURE_ENUM_SECRET\0local\0file:/owned/config\0gpg.program\nenv SIGNING_TOKEN=FIXTURE_SIGNING_SECRET gpg\0",
    );
    let rows = settings_rows(&all).join("\n");
    for secret in [
        "FIXTURE_SECRET",
        "FIXTURE_ENUM_SECRET",
        "FIXTURE_SIGNING_SECRET",
        "nvim",
        "env ",
    ] {
        assert!(!rows.contains(secret));
        assert!(!issue_url(&rows).contains(secret));
    }
    assert!(rows.contains("core.editor = (configured; value withheld)"));
    assert!(!issue_url("panic token=FIXTURE_PANIC_SECRET").contains("FIXTURE_PANIC_SECRET"));
}
