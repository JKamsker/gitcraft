use super::*;
use crate::package::display::ToolFailure;

#[test]
fn actual_finish_branch_provenance_distinguishes_tool_stderr_from_application_messages() {
    let raw_missing = missing("cargo");
    let raw_failed = finish("cargo", false, b"", b"", false).expect_err("failed");
    let raw_not_found = finish("cargo", false, b"", b"ignored", true).expect_err("missing");
    for (locale, failed) in [
        ("en-US", "cargo failed"),
        ("de", "cargo ist fehlgeschlagen"),
    ] {
        crate::i18n::set_language(locale);
        assert_eq!(ToolFailure::Failed { program: "cargo" }.display(), failed);
        let shown = ToolFailure::Missing { program: "cargo" }.display();
        assert_eq!(shown == raw_missing, locale == "en-US");
        assert_eq!(raw_missing, raw_not_found);
        // Exact byte-equal lookalikes are still external on the stderr branch.
        for said in [&raw_missing, &raw_failed] {
            let raw = finish("cargo", false, b"", said.as_bytes(), false).expect_err("stderr");
            assert_eq!(&raw, said);
            assert_eq!(ToolFailure::Opaque(&raw).display(), raw);
        }
        let out = finish("cargo", true, b"1234.5/ref/main\n", b"", false).expect("success");
        assert_eq!(out, "1234.5/ref/main\n");
    }
    crate::i18n::set_language("en-US");
}
