use super::*;
#[test]
fn successful_tls_followed_by_auth_failure_is_not_a_certificate_failure() {
    for code in [401, 403] {
        let report = classify(
            false,
            &format!(
                "Connected to fixture.invalid\nServer certificate: CN fixture.invalid\nSSL certificate verify ok\n<= Recv header: HTTP/2 {code}\nfatal: the requested URL returned error: {code}"
            ),
            true,
            std::time::Duration::ZERO,
        );
        assert!(report.lines[2].contains("completed"), "{:?}", report.lines);
        assert!(report.lines[3].ends_with("failed"), "{:?}", report.lines);
        assert!(
            report
                .lines
                .iter()
                .any(|line| line.starts_with("Authentication: the server"))
        );
    }
}
#[test]
fn later_connection_failure_preserves_earlier_tls_evidence() {
    let report = classify(
        false,
        "SSL certificate verify ok\nRecv failure: Connection reset by peer",
        true,
        std::time::Duration::ZERO,
    );
    assert!(report.lines[1].ends_with("failed"));
    assert!(report.lines[2].contains("completed earlier"));
    let report = classify(
        false,
        "Connected to fixture.invalid\nSSL certificate problem: self-signed certificate",
        true,
        std::time::Duration::ZERO,
    );
    assert!(report.lines[2].ends_with("failed"));
    assert!(!report.lines[3].ends_with("failed"));
}
