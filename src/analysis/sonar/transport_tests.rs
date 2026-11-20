//! The live transport against a loopback HTTP server: headers, status mapping, redirects,
//! timeouts, cancellation, and that no failure can carry the token.
use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::Duration;

use super::super::Failure;
use super::client::Auth;
use super::transport::{may_use_proxy, sender};

const TOKEN: &str = "squ_NEVER_SHOWN_0123456789";

#[test]
fn only_https_to_another_machine_may_go_through_a_proxy() {
    for direct in [
        "http://localhost:9000",
        "http://LOCALHOST:9000/sonar",
        "http://127.0.0.1:9000",
        "http://127.8.9.10",
        "http://[::1]:9000",
        "https://localhost:9443",
        "https://127.0.0.1",
        "http://sonar.example.com",
        "not a url",
    ] {
        assert!(!may_use_proxy(direct), "{direct}");
    }
    for proxied in [
        "https://sonar.example.com",
        "https://example.com/sonarqube",
        "https://sonarcloud.io",
        "https://10.0.0.5:9000",
    ] {
        assert!(may_use_proxy(proxied), "{proxied}");
    }
}

/// Serves `answers` to consecutive connections and returns what each request said.
fn serve(answers: Vec<String>) -> Result<(String, std::thread::JoinHandle<Vec<String>>), String> {
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    let base = format!(
        "http://127.0.0.1:{}",
        listener.local_addr().map_err(|e| e.to_string())?.port()
    );
    let handle = std::thread::spawn(move || {
        let mut seen = Vec::new();
        for answer in answers {
            let Ok((mut stream, _)) = listener.accept() else {
                break;
            };
            let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
            let mut request = Vec::new();
            let mut buffer = [0_u8; 1024];
            while !request.ends_with(b"\r\n\r\n") {
                match stream.read(&mut buffer) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => request.extend_from_slice(&buffer[..n]),
                }
            }
            seen.push(String::from_utf8_lossy(&request).into_owned());
            if answer.is_empty() {
                // Never answer: the client must time out.
                std::thread::sleep(Duration::from_millis(1500));
            } else {
                let _ = stream.write_all(answer.as_bytes());
            }
        }
        seen
    });
    Ok((base, handle))
}

fn response(status: &str, headers: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n{headers}\r\n{body}",
        body.len()
    )
}

#[test]
fn tokens_go_as_bearer_or_basic_login_and_only_where_asked() -> Result<(), String> {
    let answers = vec![
        response("200 OK", "Content-Type: text/plain\r\n", "10.4.1.88267"),
        response("200 OK", "", r#"{"valid":true}"#),
        response("200 OK", "", r#"{"valid":true}"#),
    ];
    let (base, server) = serve(answers)?;
    let mut send = sender(base, TOKEN.into(), Duration::from_secs(5));
    assert_eq!(
        send("api/server/version", Auth::None),
        Ok("10.4.1.88267".into())
    );
    assert!(send("api/authentication/validate", Auth::Bearer).is_ok());
    assert!(send("api/authentication/validate", Auth::Basic).is_ok());
    let seen = server.join().map_err(|_| "server panicked")?;
    assert!(seen[0].starts_with("GET /api/server/version HTTP/1.1"));
    assert!(!seen[0].to_ascii_lowercase().contains("authorization"));
    assert!(seen[1].contains(&format!("Bearer {TOKEN}")));
    use base64::Engine;
    let basic = base64::engine::general_purpose::STANDARD.encode(format!("{TOKEN}:"));
    assert!(seen[2].contains(&format!("Basic {basic}")));
    Ok(())
}

#[test]
fn statuses_redirects_and_rate_limits_map_to_typed_failures_without_the_token() -> Result<(), String>
{
    let echo = format!("token was {TOKEN}");
    let answers = vec![
        response("401 Unauthorized", "", &echo),
        response("403 Forbidden", "", &echo),
        response("404 Not Found", "", &echo),
        response("429 Too Many Requests", "Retry-After: 30\r\n", &echo),
        response("500 Internal Server Error", "", &echo),
        response("302 Found", "Location: https://evil.example/steal\r\n", ""),
    ];
    let (base, server) = serve(answers)?;
    let mut send = sender(base, TOKEN.into(), Duration::from_secs(5));
    let expected = [
        Failure::Unauthorized,
        Failure::Forbidden,
        Failure::Http(404),
        Failure::RateLimited(Some(30)),
        Failure::Http(500),
        Failure::Redirected,
    ];
    for want in expected {
        let got = send("api/qualitygates/project_status?projectKey=k", Auth::Bearer);
        assert_eq!(got, Err(want.clone()));
        let shown = format!("{:?} {}", want, want.text());
        assert!(!shown.contains(TOKEN), "{shown}");
    }
    let seen = server.join().map_err(|_| "server panicked")?;
    assert_eq!(seen.len(), 6, "the redirect was not followed");
    Ok(())
}

#[test]
fn a_silent_server_times_out_and_cancellation_sends_nothing() -> Result<(), String> {
    let (base, server) = serve(vec![String::new()])?;
    let mut send = sender(base.clone(), TOKEN.into(), Duration::from_millis(300));
    assert_eq!(
        send("api/server/version", Auth::None),
        Err(Failure::Timeout)
    );
    server.join().map_err(|_| "server panicked")?;
    let cancel = crate::tasks::Cancellation::default();
    cancel.cancel();
    let mut cancelled = crate::tasks::with_cancellation(cancel, || {
        sender(base, TOKEN.into(), Duration::from_secs(1))
    });
    assert_eq!(
        cancelled("api/server/version", Auth::None),
        Err(Failure::Cancelled)
    );
    Ok(())
}

#[test]
fn unsafe_paths_and_unreachable_servers_are_refused() -> Result<(), String> {
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    drop(listener);
    let mut send = sender(
        format!("http://127.0.0.1:{port}"),
        TOKEN.into(),
        Duration::from_secs(2),
    );
    for path in [
        "/api/x",
        "https://evil.example/api",
        "api/x\r\nX: y",
        "other",
    ] {
        assert_eq!(send(path, Auth::Bearer), Err(Failure::Malformed), "{path}");
    }
    assert_eq!(
        send("api/server/version", Auth::None),
        Err(Failure::Unreachable)
    );
    Ok(())
}
