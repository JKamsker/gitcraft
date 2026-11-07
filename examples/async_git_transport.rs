//! Experimental, loopback-only smart-HTTP discovery. Never used by desktop operations.
//! See docs/async-git-transport.md for the deliberately unsupported production requirements.

use std::net::SocketAddr;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const LIMIT: usize = 1024 * 1024;

async fn discover(address: SocketAddr, deadline: Duration) -> Result<Vec<String>, String> {
    if !address.ip().is_loopback() {
        return Err("the transport experiment accepts loopback addresses only".into());
    }
    tokio::time::timeout(deadline, async {
        let mut socket = tokio::net::TcpStream::connect(address)
            .await
            .map_err(|e| e.to_string())?;
        let request = format!(
            "GET /repo.git/info/refs?service=git-upload-pack HTTP/1.1\r\nHost: {address}\r\nGit-Protocol: version=2\r\nConnection: close\r\n\r\n"
        );
        socket.write_all(request.as_bytes()).await.map_err(|e| e.to_string())?;
        let mut bytes = Vec::new();
        socket.take((LIMIT + 1) as u64).read_to_end(&mut bytes).await.map_err(|e| e.to_string())?;
        if bytes.len() > LIMIT {
            return Err("discovery response exceeds 1 MiB".into());
        }
        parse_response(&bytes)
    })
    .await
    .map_err(|_| "discovery timed out; its socket was dropped".to_owned())?
}

fn parse_response(bytes: &[u8]) -> Result<Vec<String>, String> {
    let split = bytes
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or("missing HTTP headers")?;
    let headers = std::str::from_utf8(&bytes[..split]).map_err(|e| e.to_string())?;
    let mut lines = headers.split("\r\n");
    if lines.next() != Some("HTTP/1.1 200 OK") {
        return Err(
            "discovery requires HTTP/1.1 200 OK; redirects and authentication are unsupported"
                .into(),
        );
    }
    let mut content_type = None;
    let mut length = None;
    for line in lines {
        let (key, value) = line.split_once(':').ok_or("malformed HTTP header")?;
        match key.to_ascii_lowercase().as_str() {
            "content-type" => content_type = Some(value.trim()),
            "content-length" if length.is_none() => {
                length = Some(value.trim().parse::<usize>().map_err(|e| e.to_string())?);
            }
            "content-length" | "transfer-encoding" => return Err("unsupported HTTP framing".into()),
            _ => {}
        }
    }
    let body = &bytes[split + 4..];
    if content_type != Some("application/x-git-upload-pack-advertisement")
        || length != Some(body.len())
    {
        return Err("invalid discovery content type or length".into());
    }
    let mut rest = body;
    let mut packets = Vec::new();
    let mut flushed = false;
    while !rest.is_empty() {
        let header = rest.get(..4).ok_or("truncated packet header")?;
        let count =
            usize::from_str_radix(std::str::from_utf8(header).map_err(|e| e.to_string())?, 16)
                .map_err(|e| e.to_string())?;
        rest = &rest[4..];
        if count == 0 {
            flushed = true;
            continue;
        }
        if !(4..=65520).contains(&count) {
            return Err("invalid advertisement packet length".into());
        }
        let packet = rest
            .get(..count - 4)
            .ok_or("truncated advertisement packet")?;
        packets.push(
            std::str::from_utf8(packet)
                .map_err(|e| e.to_string())?
                .trim_end_matches('\n')
                .to_owned(),
        );
        rest = &rest[count - 4..];
        flushed = false;
    }
    if !flushed || packets.first().map(String::as_str) != Some("version 2") {
        return Err("expected a flushed Git protocol v2 capability advertisement".into());
    }
    Ok(packets)
}

fn main() -> Result<(), String> {
    let address: SocketAddr = std::env::args()
        .nth(1)
        .ok_or("expected a loopback IP:port")?
        .parse()
        .map_err(|e: std::net::AddrParseError| e.to_string())?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    for capability in runtime.block_on(discover(address, Duration::from_secs(5)))? {
        println!("{capability}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response(body: &[u8]) -> Vec<u8> {
        let mut bytes = format!("HTTP/1.1 200 OK\r\nContent-Type: application/x-git-upload-pack-advertisement\r\nContent-Length: {}\r\n\r\n", body.len()).into_bytes();
        bytes.extend(body);
        bytes
    }

    #[tokio::test(flavor = "current_thread")]
    async fn discovers_real_git_advertisement_without_blocking_the_runtime() -> Result<(), String> {
        // Synchronous fixture setup happens before the network work; it is not an async Git API.
        let repo =
            std::env::temp_dir().join(format!("gitcraft-async-probe-{}", std::process::id()));
        let result = std::process::Command::new("git")
            .args(["init", "--bare"])
            .arg(&repo)
            .output()
            .map_err(|e| e.to_string())?;
        assert!(result.status.success());
        let advertisement = std::process::Command::new("git")
            .env("GIT_PROTOCOL", "version=2")
            .args(["upload-pack", "--stateless-rpc", "--advertise-refs"])
            .arg(&repo)
            .output()
            .map_err(|e| e.to_string())?;
        let _ = std::fs::remove_dir_all(&repo);
        assert!(advertisement.status.success());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|e| e.to_string())?;
        let address = listener.local_addr().map_err(|e| e.to_string())?;
        let server = async {
            let (mut socket, _) = listener.accept().await.map_err(|e| e.to_string())?;
            let mut request = vec![0; 4096];
            let read = socket.read(&mut request).await.map_err(|e| e.to_string())?;
            assert!(
                String::from_utf8_lossy(&request[..read]).contains("Git-Protocol: version=2\r\n")
            );
            // Both futures execute on the same thread: a blocking client would deadlock here.
            tokio::time::sleep(Duration::from_millis(20)).await;
            for chunk in response(&advertisement.stdout).chunks(7) {
                socket.write_all(chunk).await.map_err(|e| e.to_string())?;
                tokio::task::yield_now().await;
            }
            Ok::<_, String>(())
        };
        let (capabilities, served) =
            tokio::join!(discover(address, Duration::from_secs(2)), server);
        served?;
        let capabilities = capabilities?;
        assert_eq!(capabilities.first().map(String::as_str), Some("version 2"));
        assert!(capabilities.iter().any(|c| c.starts_with("ls-refs")));
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn deadline_closes_the_connection() -> Result<(), String> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|e| e.to_string())?;
        let address = listener.local_addr().map_err(|e| e.to_string())?;
        let server = async {
            let (mut socket, _) = listener.accept().await.map_err(|e| e.to_string())?;
            let mut request = Vec::new();
            // EOF must follow timeout even though the server never sends its response.
            tokio::time::timeout(Duration::from_secs(2), socket.read_to_end(&mut request))
                .await
                .map_err(|e| e.to_string())?
                .map_err(|e| e.to_string())?;
            assert!(!request.is_empty());
            Ok::<_, String>(())
        };
        let (result, served) = tokio::join!(discover(address, Duration::from_millis(50)), server);
        assert!(result.is_err_and(|e| e.contains("timed out")));
        served
    }

    #[test]
    fn refuses_truncated_framing_and_non_git_responses() {
        assert!(parse_response(&response(b"000eversion 2\n0000")).is_ok());
        for body in [
            b"000eversion 2\n".as_slice(),
            b"ffffshort0000",
            b"0003",
            b"0000",
            b"not git",
        ] {
            assert!(parse_response(&response(body)).is_err());
        }
        assert!(
            parse_response(b"HTTP/1.1 302 Found\r\nLocation: https://elsewhere/\r\n\r\n").is_err()
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn refuses_production_addresses_and_bounds_response_memory() -> Result<(), String> {
        assert!(
            discover(
                "192.0.2.1:80"
                    .parse()
                    .map_err(|e: std::net::AddrParseError| e.to_string())?,
                Duration::from_secs(1)
            )
            .await
            .is_err_and(|e| e.contains("loopback"))
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|e| e.to_string())?;
        let address = listener.local_addr().map_err(|e| e.to_string())?;
        let server = async {
            let (mut socket, _) = listener.accept().await.map_err(|e| e.to_string())?;
            let mut request = [0; 4096];
            socket.read(&mut request).await.map_err(|e| e.to_string())?;
            socket
                .write_all(&vec![b'x'; LIMIT + 1])
                .await
                .map_err(|e| e.to_string())
        };
        let (result, served) = tokio::join!(discover(address, Duration::from_secs(2)), server);
        served?;
        assert!(result.is_err_and(|e| e.contains("exceeds 1 MiB")));
        Ok(())
    }
}
