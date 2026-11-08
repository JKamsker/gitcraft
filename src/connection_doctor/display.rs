//! Finite paint adapters. Inventory, trace classification, credentials and raw lines stay unchanged.
pub(crate) fn line(raw: &str) -> String {
    let id = match raw {
        "Invalid network configuration response" => Some("interaction-doctor-invalid-config"),
        "TLS chain: NOT verified — certificate verification is disabled in Git's configuration or environment" => {
            Some("interaction-doctor-tls-unverified")
        }
        "TLS chain: not used for an unencrypted HTTP remote" => {
            Some("interaction-doctor-http-unencrypted")
        }
        "Git GIT_SSL_NO_VERIFY: certificate verification disabled" => {
            Some("interaction-doctor-ssl-env")
        }
        "Git SSH environment override: configured (command withheld; may override SSH config)" => {
            Some("interaction-doctor-ssh-env")
        }
        "GitHub / GitLab CLI clients use their own CLI configuration and environment." => {
            Some("interaction-doctor-cli-config")
        }
        "SSH ProxyJump / ProxyCommand: ssh -G unavailable; inspect SSH config on the repository machine" => {
            Some("interaction-doctor-ssh-unavailable")
        }
        "SSH ProxyJump / ProxyCommand: applies to SSH remotes" => {
            Some("interaction-doctor-ssh-remotes")
        }
        "The selected endpoint no longer exists" => Some("interaction-doctor-endpoint-gone"),
        "Connection tests read the fetch route. Push sign-ins can be inspected and rejected on the HTTPS sign-ins page; write permissions are checked by the actual push." => {
            Some("interaction-doctor-fetch-only")
        }
        "Choose an HTTPS, HTTP or SSH remote to test a network connection" => {
            Some("interaction-doctor-network-only")
        }
        "407: the proxy wants a password" => Some("interaction-doctor-proxy-auth"),
        "DNS: the host or proxy could not be resolved" => Some("interaction-doctor-dns-failed"),
        "SSH host verification failed; verify the server's host key on the repository machine" => {
            Some("interaction-doctor-ssh-host")
        }
        "TLS: certificate chain or handshake failed; check the configured CA and hostname" => {
            Some("interaction-doctor-tls-failed")
        }
        "Authentication: the server refused this account or no unattended sign-in is available" => {
            Some("interaction-doctor-auth-failed")
        }
        "TCP: could not connect to the server or proxy, or the connection failed" => {
            Some("interaction-doctor-tcp-failed")
        }
        "Git could not complete ls-remote; inspect the repository operation log for details. No transport stage can be safely inferred." => {
            Some("interaction-doctor-unknown-trace")
        }
        _ => None,
    };
    if let Some(id) = id {
        return crate::i18n::text(id).to_owned();
    }
    if let Some((stage, state)) = raw.split_once(": ") {
        let stage = match stage {
            "DNS" => Some("interaction-doctor-stage-dns"),
            "TCP" => Some("interaction-doctor-stage-tcp"),
            "TLS chain (Git's configured CA trust)" => Some("interaction-doctor-stage-tls"),
            "SSH transport handshake" => Some("interaction-doctor-stage-ssh"),
            "Authentication" => Some("interaction-doctor-stage-authentication"),
            _ => None,
        };
        let state = match state {
            "accepted for this read (the repository may be public)" => {
                Some("interaction-doctor-state-accepted")
            }
            "completed through the configured transport" => {
                Some("interaction-doctor-state-complete")
            }
            "completed earlier in this trace" => Some("interaction-doctor-state-earlier"),
            "completed before the failure" => Some("interaction-doctor-state-before-failure"),
            "failed" => Some("interaction-doctor-state-failed"),
            "not confirmed" => Some("interaction-doctor-state-unconfirmed"),
            _ => None,
        };
        if let (Some(stage), Some(state)) = (stage, state) {
            return crate::i18n::message(
                "interaction-doctor-stage-state",
                &[
                    ("stage", crate::i18n::text(stage).into()),
                    ("state", crate::i18n::text(state).into()),
                ],
            );
        }
    }
    if let Some(value) = raw.strip_prefix("Git http.proxy: ") {
        return crate::i18n::message(
            "interaction-doctor-proxy",
            &[("value", display_value(value).into())],
        );
    }
    if let Some(value) = raw.strip_prefix("Git remote proxy override: ") {
        return crate::i18n::message(
            "interaction-doctor-remote-proxy",
            &[("value", display_value(value).into())],
        );
    }
    if let Some(value) = raw.strip_prefix("Forge REST client (this computer): ") {
        return crate::i18n::message(
            "interaction-doctor-rest",
            &[("value", display_value(value).into())],
        );
    }
    if let Some(value) = raw.strip_prefix("SSH command override: ") {
        return crate::i18n::message(
            "interaction-doctor-ssh-command",
            &[("value", display_value(value).into())],
        );
    }
    if let Some(value) = raw.strip_prefix("SSH proxyjump: ") {
        return crate::i18n::message(
            "interaction-doctor-ssh-jump",
            &[("value", display_value(value).into())],
        );
    }
    if let Some(value) = raw.strip_prefix("SSH proxycommand: ") {
        return crate::i18n::message(
            "interaction-doctor-ssh-proxy",
            &[("value", display_value(value).into())],
        );
    }
    if let Some((key, value)) = raw.split_once(": ") {
        if [
            "http.sslCAInfo",
            "http.sslCAPath",
            "http.sslBackend",
            "http.sslVerify",
            "http.proxySSLVerify",
        ]
        .contains(&key)
            || [
                "Git https_proxy",
                "Git HTTPS_PROXY",
                "Git http_proxy",
                "Git HTTP_PROXY",
                "Git all_proxy",
                "Git ALL_PROXY",
                "Git no_proxy",
                "Git NO_PROXY",
            ]
            .contains(&key)
        {
            return crate::i18n::message(
                "interaction-doctor-key-value",
                &[("key", key.into()), ("value", display_value(value).into())],
            );
        }
    }
    for (prefix, id) in [
        ("ls-remote: completed in ", "interaction-doctor-elapsed-ok"),
        ("ls-remote: failed in ", "interaction-doctor-elapsed-failed"),
    ] {
        if let Some(number) = raw.strip_prefix(prefix).and_then(|s| s.strip_suffix(" s")) {
            if let Ok(seconds) = number.parse::<f64>() {
                if seconds.is_finite() && seconds >= 0.0 && format!("{seconds:.2}") == number {
                    return crate::i18n::message(id, &[("seconds", seconds.into())]);
                }
            }
        }
    }
    let remote = crate::remote::display::diagnostic(raw);
    if remote == raw {
        crate::git::display_error(raw)
    } else {
        remote
    }
}

fn display_value(raw: &str) -> String {
    let id = match raw {
        "none" => Some("interaction-doctor-value-none"),
        "configured (value withheld)" => Some("interaction-doctor-value-withheld"),
        "not configured; environment applies" => Some("interaction-doctor-value-proxy-env"),
        "explicitly disabled" => Some("interaction-doctor-value-disabled"),
        "direct (including NO_PROXY bypass)" => Some("interaction-doctor-value-direct"),
        "Git default" => Some("interaction-doctor-value-default"),
        "custom CA location configured" => Some("interaction-doctor-value-ca"),
        "configured (command withheld; may override SSH config)" => {
            Some("interaction-doctor-value-command")
        }
        "configured bypass list" => Some("interaction-doctor-value-bypass"),
        _ => None,
    };
    id.map(|id| crate::i18n::text(id).to_owned())
        .unwrap_or_else(|| raw.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actual_trace_classification_keeps_raw_success_and_stages_while_rendering_in_selected_language()
     {
        let trace = "Connected to fixture.invalid\nSSL certificate verify ok\nHTTP/2 403\nAuthorization: FIXTURE_SECRET";
        let report =
            super::super::classify(false, trace, true, std::time::Duration::from_millis(2340));
        let raw = report.lines.clone();
        assert!(!report.success);
        assert!(raw[2].contains("completed earlier"));
        assert!(raw[3].ends_with("failed"));
        crate::i18n::set_language("de");
        let translated: Vec<_> = raw.iter().map(|s| line(s)).collect();
        assert_eq!(
            translated[2],
            "TLS-Zertifikatskette (von Git konfigurierte CA-Vertrauensstellung): zuvor in diesem Ablauf abgeschlossen"
        );
        assert_eq!(translated[3], "Authentifizierung: fehlgeschlagen");
        assert!(translated.iter().any(|s|s=="Authentifizierung: Der Server hat dieses Konto abgelehnt oder es ist keine unbeaufsichtigte Anmeldung verfügbar"));
        assert_eq!(
            translated.last().map(String::as_str),
            Some("ls-remote: nach 2,34 s fehlgeschlagen")
        );
        assert!(!translated.join(" ").contains("FIXTURE_SECRET"));
        assert_eq!(report.lines, raw);
        assert!(!report.success);
        assert_eq!(
            line(
                "TLS chain: NOT verified — certificate verification is disabled in Git's configuration or environment"
            ),
            "TLS-Zertifikatskette: NICHT geprüft — die Zertifikatsprüfung ist in der Git-Konfiguration oder -Umgebung deaktiviert"
        );
        assert_eq!(
            line("TLS chain: not used for an unencrypted HTTP remote"),
            "TLS-Zertifikatskette: bei einem unverschlüsselten HTTP-Remote nicht verwendet"
        );
        let success =
            super::super::classify(true, "opaque trace", false, std::time::Duration::ZERO);
        assert!(success.success);
        assert_eq!(
            line(&success.lines[2]),
            "SSH-Transport-Handshake: über den konfigurierten Transport abgeschlossen"
        );
        assert_eq!(
            line(&success.lines[3]),
            "Authentifizierung: für diesen Lesezugriff akzeptiert (das Repository ist möglicherweise öffentlich)"
        );
        crate::i18n::set_language("en-US");
        for raw in raw.iter().chain(&success.lines) {
            assert_eq!(line(raw), *raw);
        }
    }

    #[test]
    fn actual_redacted_endpoint_and_finite_inventory_values_remain_opaque_and_known_units_format_locally()
     {
        let endpoint =
            super::super::endpoint("https://user:FIXTURE_SECRET@proxy.example/path?q=token");
        assert_eq!(endpoint, "https://proxy.example");
        crate::i18n::set_language("de");
        assert_eq!(
            line(&format!("Forge REST client (this computer): {endpoint}")),
            format!("Forge-REST-Client (dieser Rechner): {endpoint}")
        );
        let cases = [
            (
                "Git http.proxy: not configured; environment applies",
                "Git http.proxy: nicht konfiguriert; die Umgebung gilt",
            ),
            (
                "Git remote proxy override: explicitly disabled",
                "Git-Remote-Proxyüberschreibung: ausdrücklich deaktiviert",
            ),
            (
                "Git no_proxy: configured bypass list",
                "Git no_proxy: Ausnahmeliste konfiguriert",
            ),
            (
                "SSH command override: configured (command withheld; may override SSH config)",
                "SSH-Befehlsüberschreibung: konfiguriert (Befehl zurückgehalten; kann die SSH-Konfiguration überschreiben)",
            ),
            ("SSH proxyjump: none", "SSH proxyjump: keine"),
            (
                "http.sslCAInfo: custom CA location configured",
                "http.sslCAInfo: benutzerdefinierter CA-Speicherort konfiguriert",
            ),
        ];
        for (raw, de) in cases {
            assert_eq!(line(raw), de);
        }
        assert_eq!(
            line("ls-remote: completed in 1234.50 s"),
            "ls-remote: in 1.234,50 s abgeschlossen"
        );
        for unknown in [
            "opaque external 日本語",
            "DNS: unknown external detail",
            "ls-remote: completed in NaN s",
            "ls-remote: completed in 00.12 s",
            "unknown.key: Git default",
        ] {
            assert_eq!(line(unknown), unknown);
        }
        crate::i18n::set_language("en-US");
        for (raw, _) in cases {
            assert_eq!(line(raw), raw);
        }
        assert_eq!(endpoint, "https://proxy.example");
    }
}
