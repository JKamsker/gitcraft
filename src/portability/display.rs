//! Finite setup-family presentation. Producers, validation and retry identities stay raw.
use crate::i18n::{message as format, text};

pub(crate) fn message(raw: &str) -> String {
    let id = match raw {
        "Setup worker stopped; retained destinations are available for inspection" => {
            "interaction-setup-status-setup-worker-stopped-retained-destinations-are-available-for-insp"
        }
        "Setup operation cancelled; stale results were discarded" => {
            "interaction-setup-status-setup-operation-cancelled-stale-results-were-discarded"
        }
        "Preview ready. Only successfully read selected repositories are included; review the count and omissions before saving." => {
            "interaction-setup-status-preview-ready-only-successfully-read-selected-repositories-are-in"
        }
        "Preview loaded. No settings, repositories or authentication have been changed." => {
            "interaction-setup-status-preview-loaded-no-settings-repositories-or-authentication-have-be"
        }
        "Profile setup verified. Activate it explicitly before cloning its repositories." => {
            "interaction-setup-status-profile-setup-verified-activate-it-explicitly-before-cloning-its"
        }
        "Profile changed during verification; check the current profile again." => {
            "interaction-setup-status-profile-changed-during-verification-check-the-current-profile-aga"
        }
        "Load an import preview first" => "interaction-setup-status-load-an-import-preview-first",
        "Profile ID space is exhausted" => "interaction-setup-status-profile-id-space-is-exhausted",
        "Public settings and new profiles added. Your active profile is unchanged. Authentication, keys and signing programs require local setup." => {
            "interaction-setup-status-public-settings-and-new-profiles-added-your-active-profile-is-unc"
        }
        "Previous clone/worktrees were retained. Choose a new unused destination." => {
            "interaction-setup-status-previous-clone-worktrees-were-retained-choose-a-new-unused-destin"
        }
        "Import cancelled" => "interaction-setup-status-import-cancelled",
        "Export cancelled" => "interaction-setup-status-export-cancelled",
        "Choose a regular setup file of at most 8 MiB" => {
            "interaction-setup-status-choose-a-regular-setup-file-of-at-most-8-mib"
        }
        "Select a local absolute SSH key path" => {
            "interaction-setup-status-select-a-local-absolute-ssh-key-path"
        }
        "Selected local SSH key is unavailable" => {
            "interaction-setup-status-selected-local-ssh-key-is-unavailable"
        }
        "Selected SSH key must be a local file" => {
            "interaction-setup-status-selected-ssh-key-must-be-a-local-file"
        }
        "Profile is no longer pending" => "interaction-setup-status-profile-is-no-longer-pending",
        "Choose this machine's signing key" => {
            "interaction-setup-status-choose-this-machine-s-signing-key"
        }
        "Profile setup cancelled" => "interaction-setup-status-profile-setup-cancelled",
        "The expected local tea login is unavailable" => {
            "interaction-setup-status-the-expected-local-tea-login-is-unavailable"
        }
        "The tea login belongs to a different HTTPS server" => {
            "interaction-setup-status-the-tea-login-belongs-to-a-different-https-server"
        }
        "The imported tea expectation has no public owner; export it again from the original machine" => {
            "interaction-setup-status-the-imported-tea-expectation-has-no-public-owner-export-it-again"
        }
        "The selected tea alias belongs to a different account user" => {
            "interaction-setup-status-the-selected-tea-alias-belongs-to-a-different-account-user"
        }
        "A portable URL needs a repository path" => {
            "interaction-setup-status-a-portable-url-needs-a-repository-path"
        }
        "Invalid remote port" => "interaction-setup-status-invalid-remote-port",
        "Invalid repository URL path" => "interaction-setup-status-invalid-repository-url-path",
        "Invalid SSH transport username" => {
            "interaction-setup-status-invalid-ssh-transport-username"
        }
        "Use an explicit SSH username in scp-style URLs" => {
            "interaction-setup-status-use-an-explicit-ssh-username-in-scp-style-urls"
        }
        "Invalid scp-style repository URL" => {
            "interaction-setup-status-invalid-scp-style-repository-url"
        }
        "Only credential-free HTTPS and SSH remote URLs can be imported; local sources must be selected separately on this machine" => {
            "interaction-setup-status-only-credential-free-https-and-ssh-remote-urls-can-be-imported-lo"
        }
        "Import receipt/config was replaced or exceeds limits" => {
            "interaction-setup-status-import-receipt-config-was-replaced-or-exceeds-limits"
        }
        "Import receipt/config exceeds limits" => {
            "interaction-setup-status-import-receipt-config-exceeds-limits"
        }
        "Import definition changed; use a new destination" => {
            "interaction-setup-status-import-definition-changed-use-a-new-destination"
        }
        "The import repository path was replaced" => {
            "interaction-setup-status-the-import-repository-path-was-replaced"
        }
        "The import clone/config changed; finish manually instead of retrying" => {
            "interaction-setup-status-the-import-clone-config-changed-finish-manually-instead-of-retryi"
        }
        "Worktree path is not UTF-8" => "interaction-setup-status-worktree-path-is-not-utf-8",
        "Import targets must be local absolute folders on this machine" => {
            "interaction-setup-status-import-targets-must-be-local-absolute-folders-on-this-machine"
        }
        "Choose a destination with a UTF-8 path" => {
            "interaction-setup-status-choose-a-destination-with-a-utf-8-path"
        }
        "Retry destination changed" => "interaction-setup-status-retry-destination-changed",
        "Choose a local absolute source folder on this machine" => {
            "interaction-setup-status-choose-a-local-absolute-source-folder-on-this-machine"
        }
        "Choose a local source with a UTF-8 path" => {
            "interaction-setup-status-choose-a-local-source-with-a-utf-8-path"
        }
        "No portable remote; select a local source folder on this machine" => {
            "interaction-setup-status-no-portable-remote-select-a-local-source-folder-on-this-machine"
        }
        "Missing import receipt" => "interaction-setup-status-missing-import-receipt",
        "Detached commit is unavailable; no substitute was used" => {
            "interaction-setup-status-detached-commit-is-unavailable-no-substitute-was-used"
        }
        "The named branch uses the selected source's current tip. Local-only commits and working files are not included in setup files." => {
            "interaction-setup-status-the-named-branch-uses-the-selected-source-s-current-tip-local-onl"
        }
        "Import cancelled; completed clone/worktrees were retained" => {
            "interaction-setup-status-import-cancelled-completed-clone-worktrees-were-retained"
        }
        "Worktree destination root was replaced" => {
            "interaction-setup-status-worktree-destination-root-was-replaced"
        }
        "Selected checkout, remotes, author and worktree steps completed. Review any notices above." => {
            "interaction-setup-status-selected-checkout-remotes-author-and-worktree-steps-completed-rev"
        }
        "Repository metadata is not UTF-8" => {
            "interaction-setup-status-repository-metadata-is-not-utf-8"
        }
        "More than 32 remotes; narrow the repository setup first" => {
            "interaction-setup-status-more-than-32-remotes-narrow-the-repository-setup-first"
        }
        "More than 16 push URLs; narrow the repository setup first" => {
            "interaction-setup-status-more-than-16-push-urls-narrow-the-repository-setup-first"
        }
        "A bare worktree descriptor was omitted" => {
            "interaction-setup-status-a-bare-worktree-descriptor-was-omitted"
        }
        "More than 64 linked worktrees; narrow the export" => {
            "interaction-setup-status-more-than-64-linked-worktrees-narrow-the-export"
        }
        "Cannot export the selected tea account: local public ownership is unavailable" => {
            "interaction-setup-status-cannot-export-the-selected-tea-account-local-public-ownership-is"
        }
        "The selected tea alias has a different host or no public owner; check local profile setup before export" => {
            "interaction-setup-status-the-selected-tea-alias-has-a-different-host-or-no-public-owner-ch"
        }
        "Setup file exceeds 8 MiB" => "interaction-setup-status-setup-file-exceeds-8-mib",
        "Unsupported setup format/version" => {
            "interaction-setup-status-unsupported-setup-format-version"
        }
        "Setup contains too many profiles, identities or repositories" => {
            "interaction-setup-status-setup-contains-too-many-profiles-identities-or-repositories"
        }
        "Invalid display setting" => "interaction-setup-status-invalid-display-setting",
        "Display setting is outside supported range" => {
            "interaction-setup-status-display-setting-is-outside-supported-range"
        }
        "Invalid fold patterns" => "interaction-setup-status-invalid-fold-patterns",
        "Too many portable preference values" => {
            "interaction-setup-status-too-many-portable-preference-values"
        }
        "Invalid portable scheduling setting" => {
            "interaction-setup-status-invalid-portable-scheduling-setting"
        }
        "Duplicate profile ID" => "interaction-setup-status-duplicate-profile-id",
        "Only built-in themes can be imported" => {
            "interaction-setup-status-only-built-in-themes-can-be-imported"
        }
        "Unsupported signing format" => "interaction-setup-status-unsupported-signing-format",
        "Too many account expectations" => "interaction-setup-status-too-many-account-expectations",
        "Duplicate provider expectation" => {
            "interaction-setup-status-duplicate-provider-expectation"
        }
        "Expected account is empty" => "interaction-setup-status-expected-account-is-empty",
        "Expected account owner is empty" => {
            "interaction-setup-status-expected-account-owner-is-empty"
        }
        "A portable tea alias must include its expected public account owner" => {
            "interaction-setup-status-a-portable-tea-alias-must-include-its-expected-public-account-own"
        }
        "Duplicate repository destination name" => {
            "interaction-setup-status-duplicate-repository-destination-name"
        }
        "Repository references an unknown profile" => {
            "interaction-setup-status-repository-references-an-unknown-profile"
        }
        "Too many profile associations" => "interaction-setup-status-too-many-profile-associations",
        "Repository metadata exceeds limits" => {
            "interaction-setup-status-repository-metadata-exceeds-limits"
        }
        "Duplicate remote name" => "interaction-setup-status-duplicate-remote-name",
        "Remote URL contains credentials or nonportable decorations" => {
            "interaction-setup-status-remote-url-contains-credentials-or-nonportable-decorations"
        }
        "Too many push URLs" => "interaction-setup-status-too-many-push-urls",
        "Push URL contains credentials" => "interaction-setup-status-push-url-contains-credentials",
        "Duplicate worktree name" => "interaction-setup-status-duplicate-worktree-name",
        "Text contains control characters or exceeds its limit" => {
            "interaction-setup-status-text-contains-control-characters-or-exceeds-its-limit"
        }
        "Choose a plain, unique folder name" => {
            "interaction-setup-status-choose-a-plain-unique-folder-name"
        }
        "Invalid portable ref or remote name" => {
            "interaction-setup-status-invalid-portable-ref-or-remote-name"
        }
        "Only branch visibility refs are portable" => {
            "interaction-setup-status-only-branch-visibility-refs-are-portable"
        }
        "Invalid expected server hostname" => {
            "interaction-setup-status-invalid-expected-server-hostname"
        }
        "Invalid stored commit ID" => "interaction-setup-status-invalid-stored-commit-id",
        "Too many portable configuration values" => {
            "interaction-setup-status-too-many-portable-configuration-values"
        }
        "Invalid portable configuration response" => {
            "interaction-setup-status-invalid-portable-configuration-response"
        }
        _ => "",
    };
    if !id.is_empty() {
        return text(id).to_owned();
    }
    if let Some(detail) = raw.strip_prefix("Setup saved to ") {
        return format(
            "interaction-setup-status-saved",
            &[("detail", detail.into())],
        );
    }
    if let Some(detail) = raw.strip_prefix("Omitted ") {
        return format(
            "interaction-setup-status-omitted",
            &[("detail", detail.into())],
        );
    }
    if let Some(detail) = raw.strip_prefix("Choose a new unused setup filename: ") {
        return format(
            "interaction-setup-status-new-filename",
            &[("detail", detail.into())],
        );
    }
    if let Some(detail) = raw.strip_prefix("Writing setup failed; inspect the partial file: ") {
        return format(
            "interaction-setup-status-write-failed",
            &[("detail", detail.into())],
        );
    }
    if let Some(detail) = raw.strip_prefix("Invalid setup file: ") {
        return format(
            "interaction-setup-status-invalid-file",
            &[("detail", detail.into())],
        );
    }
    if let Some(detail) = raw.strip_prefix("Unsupported portable configuration key/value: ") {
        return format(
            "interaction-setup-status-invalid-key",
            &[("detail", detail.into())],
        );
    }
    if let Some(detail) = raw.strip_prefix("Choose a new unused destination: ") {
        return format(
            "interaction-setup-status-new-destination",
            &[("detail", detail.into())],
        );
    }
    if let Some(detail) = raw.strip_prefix("Clone failed; destination retained for inspection: ") {
        return format(
            "interaction-setup-status-clone-failed",
            &[("detail", detail.into())],
        );
    }
    if let Some(name) = raw
        .strip_prefix("Branch ")
        .and_then(|s| s.strip_suffix(" is missing; no substitute was checked out"))
        && super::reference(name).is_ok()
    {
        return format(
            "interaction-setup-status-missing-branch",
            &[("name", name.into())],
        );
    }
    if let Some(name) = raw
        .strip_prefix("Previously restored worktree ")
        .and_then(|s| s.strip_suffix(" was replaced; finish manually"))
        && super::leaf(name).is_ok()
    {
        return format(
            "interaction-setup-status-replaced-worktree",
            &[("name", name.into())],
        );
    }
    if let Some(name) = raw
        .strip_prefix("Worktree ")
        .and_then(|s| s.strip_suffix(" already exists and was not overwritten"))
        && super::leaf(name).is_ok()
    {
        return format(
            "interaction-setup-status-existing-worktree",
            &[("name", name.into())],
        );
    }
    if let Some(name) = raw
        .strip_prefix("Worktree ")
        .and_then(|s| s.strip_suffix(" commit is unavailable; no substitute was used"))
        && super::leaf(name).is_ok()
    {
        return format(
            "interaction-setup-status-missing-commit",
            &[("name", name.into())],
        );
    }
    if let Some(name) = raw.strip_suffix(
        ": local/unsupported source omitted; choose a current-machine local source on import",
    ) && super::reference(name).is_ok()
    {
        return format(
            "interaction-setup-status-omitted-source",
            &[("name", name.into())],
        );
    }
    if let Some(name) =
        raw.strip_suffix(": credential material/query/fragment removed from fetch URL")
        && super::reference(name).is_ok()
    {
        return format(
            "interaction-setup-status-fetch-cleaned",
            &[("name", name.into())],
        );
    }
    if let Some(name) = raw.strip_suffix(": credential material removed from push URL")
        && super::reference(name).is_ok()
    {
        return format(
            "interaction-setup-status-push-cleaned",
            &[("name", name.into())],
        );
    }
    if let Some(name) = raw.strip_suffix(": unsupported push URL omitted")
        && super::reference(name).is_ok()
    {
        return format(
            "interaction-setup-status-push-omitted",
            &[("name", name.into())],
        );
    }
    crate::git::display_error(raw)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portability::Setup;

    #[test]
    fn actual_setup_validation_remains_raw_while_the_real_paint_adapter_selects_german()
    -> Result<(), String> {
        crate::i18n::set_language("de");
        let mut file = Setup::new(&crate::settings::Settings::default(), &Default::default());
        file.version = 999;
        let raw = file
            .validate()
            .err()
            .ok_or("expected version validation error")?;
        assert_eq!(raw, "Unsupported setup format/version");
        assert_eq!(
            message(&raw),
            "Nicht unterstütztes Einrichtungsformat oder nicht unterstützte Version"
        );
        assert_eq!(file.version, 999);
        let raw_leaf = crate::portability::leaf("../literal")
            .err()
            .ok_or("expected invalid folder")?;
        assert_eq!(raw_leaf, "Choose a plain, unique folder name");
        assert_eq!(
            message(&raw_leaf),
            "Wähle einen einfachen, eindeutigen Ordnernamen"
        );
        let decode = Setup::decode(b"not JSON")
            .err()
            .ok_or("expected decode error")?;
        assert!(decode.starts_with("Invalid setup file: "));
        let detail = decode
            .strip_prefix("Invalid setup file: ")
            .ok_or("decode prefix")?;
        assert_eq!(
            message(&decode),
            format!("Ungültige Einrichtungsdatei: {detail}")
        );
        // Cancellation classifiers still receive their original English producer identity.
        let cancelled = "Import cancelled; completed clone/worktrees were retained";
        assert!(cancelled.contains("cancelled"));
        assert_eq!(
            message(cancelled),
            "Import abgebrochen; der fertige Klon und die Worktrees wurden beibehalten"
        );
        assert!(cancelled.contains("cancelled"));
        crate::i18n::set_language("en-US");
        assert_eq!(message(&raw), raw);
        assert_eq!(message(&raw_leaf), raw_leaf);
        assert_eq!(message(&decode), decode);
        Ok(())
    }

    #[test]
    fn known_setup_envelopes_and_notices_preserve_literal_fields_and_unknown_diagnostics() {
        crate::i18n::set_language("de");
        let cases = [
            (
                "Setup saved to /opaque/$() {literal} 日本語.json",
                "Einrichtung gespeichert unter /opaque/$() {literal} 日本語.json",
            ),
            (
                "Omitted /opaque: odd/path: External Git diagnostic 日本語",
                "Ausgelassen: /opaque: odd/path: External Git diagnostic 日本語",
            ),
            (
                "Clone failed; destination retained for inspection: External Git diagnostic { $count }",
                "Klonen fehlgeschlagen; das Ziel wurde zur Überprüfung beibehalten: External Git diagnostic { $count }",
            ),
            (
                "Branch feature/a-b is missing; no substitute was checked out",
                "Branch feature/a-b fehlt; es wurde kein Ersatz ausgecheckt",
            ),
            (
                "Previously restored worktree saved-tree was replaced; finish manually",
                "Der zuvor wiederhergestellte Worktree saved-tree wurde ersetzt; schließe die Einrichtung manuell ab",
            ),
            (
                "Worktree saved-tree already exists and was not overwritten",
                "Worktree saved-tree ist bereits vorhanden und wurde nicht überschrieben",
            ),
            (
                "Worktree saved-tree commit is unavailable; no substitute was used",
                "Der Commit von Worktree saved-tree ist nicht verfügbar; es wurde kein Ersatz verwendet",
            ),
            (
                "origin: local/unsupported source omitted; choose a current-machine local source on import",
                "origin: Lokale oder nicht unterstützte Quelle ausgelassen; wähle beim Import eine lokale Quelle auf dem Zielrechner",
            ),
            (
                "origin: credential material/query/fragment removed from fetch URL",
                "origin: Zugangsdaten, Abfrageparameter und Fragment aus der Fetch-URL entfernt",
            ),
            (
                "origin: credential material removed from push URL",
                "origin: Zugangsdaten aus der Push-URL entfernt",
            ),
            (
                "origin: unsupported push URL omitted",
                "origin: Nicht unterstützte Push-URL ausgelassen",
            ),
        ];
        for (raw, expected) in cases {
            assert_eq!(message(raw), expected);
        }
        for raw in [
            "External Git diagnostic 日本語",
            "Unsupported setup format/version: external extension",
            "Branch external: diagnostic is missing; no substitute was checked out",
            "Worktree external: diagnostic already exists and was not overwritten",
            "remote with spaces: unsupported push URL omitted",
            "origin: unsupported push URL omitted; external extension",
        ] {
            assert_eq!(message(raw), raw);
        }
        crate::i18n::set_language("en-US");
        for (raw, _) in cases {
            assert_eq!(message(raw), raw);
        }
    }
}
