//! Credential-free public HTTP. Bounded reads, deadlines, and no redirects.
use super::DiscoveryError;
use std::io::Read;
use std::time::Duration;

pub(in crate::forge) const BODY_LIMIT: usize = 2 * 1024 * 1024;
pub(in crate::forge) struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}
impl Response {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}
pub(in crate::forge) fn public_get(endpoint: &str) -> Result<Response, DiscoveryError> {
    let url = format!("https://api.github.com{endpoint}");
    request(&url)
}

fn request(url: &str) -> Result<Response, DiscoveryError> {
    let agent = super::super::http::with_proxy(ureq::AgentBuilder::new(), url)
        .timeout(Duration::from_secs(20))
        .redirects(0)
        .user_agent(concat!("gitcraft/", env!("CARGO_PKG_VERSION")))
        .build();
    let response = match agent
        .get(url)
        .set("Accept", "application/vnd.github+json")
        .set("X-GitHub-Api-Version", "2022-11-28")
        .call()
    {
        Ok(response) | Err(ureq::Error::Status(_, response)) => response,
        Err(_) => return Err(DiscoveryError::Transport),
    };
    let status = response.status();
    let headers = response
        .headers_names()
        .into_iter()
        .filter_map(|key| {
            response
                .header(&key)
                .map(|value| (key.clone(), value.to_owned()))
        })
        .collect();
    let mut body = Vec::new();
    response
        .into_reader()
        .take((BODY_LIMIT + 1) as u64)
        .read_to_end(&mut body)
        .map_err(|_| DiscoveryError::Transport)?;
    if body.len() > BODY_LIMIT {
        return Err(DiscoveryError::Malformed);
    }
    Ok(Response {
        status,
        headers,
        body,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    #[test]
    fn discovery_anonymous_sends_no_authorization() -> Result<(), Box<dyn std::error::Error>> {
        let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
        let address = listener.local_addr()?;
        listener.set_nonblocking(true)?;
        let server = std::thread::spawn(move || -> std::io::Result<String> {
            let deadline = std::time::Instant::now() + Duration::from_secs(3);
            let (mut stream, _) = loop {
                match listener.accept() {
                    Ok(connection) => break connection,
                    Err(error)
                        if error.kind() == std::io::ErrorKind::WouldBlock
                            && std::time::Instant::now() < deadline =>
                    {
                        std::thread::sleep(Duration::from_millis(10))
                    }
                    Err(error) => return Err(error),
                }
            };
            // Windows can inherit the listener's nonblocking mode on accept.
            stream.set_nonblocking(false)?;
            stream.set_read_timeout(Some(Duration::from_secs(3)))?;
            stream.set_write_timeout(Some(Duration::from_secs(3)))?;
            let mut request = Vec::new();
            let mut byte = [0];
            while !request.ends_with(b"\r\n\r\n") && request.len() < 8192 {
                if stream.read(&mut byte)? == 0 {
                    break;
                }
                request.push(byte[0]);
            }
            stream.write_all(
                b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n[]",
            )?;
            Ok(String::from_utf8_lossy(&request).to_lowercase())
        });
        let response = request(&format!("http://{address}/repos"));
        let captured = server.join().map_err(|_| "fixture panicked")??;
        assert!(response.is_ok());
        assert!(!captured.contains("authorization:"));
        assert!(!captured.contains("cookie:"));
        Ok(())
    }
}
