use super::*;
#[test]
fn fluent_partition_bakeoff_actual_kind_label_keeps_raw_classification() {
    for language in ["en-US", "de"] {
        crate::i18n::set_language(language);
        for (patch, raw, german) in [
            ("", "unchanged", "unverändert"),
            (
                "diff --git a/x b/x\ndeleted file mode 100644",
                "deleted",
                "gelöscht",
            ),
            (
                "diff --git a/x b/x\nnew file mode 100644",
                "added",
                "hinzugefügt",
            ),
            ("@@ -1 +1 @@\n-before\n+after", "changed", "geändert"),
        ] {
            let side = Side {
                patch: patch.into(),
                hunks: Vec::new(),
            };
            assert_eq!(side.kind(), raw);
            assert_eq!(
                side.display_kind(),
                if language == "de" { german } else { raw }
            );
            assert_eq!(side.kind() != "unchanged", !patch.is_empty());
            assert_eq!(side.patch, patch);
        }
    }
    crate::i18n::set_language("en-US");
}
