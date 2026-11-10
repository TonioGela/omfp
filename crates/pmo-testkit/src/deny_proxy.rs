use std::net::SocketAddr;
use std::sync::{Arc, Mutex, PoisonError};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

/// An HTTP(S) proxy that answers `403` to everything and records the request line of each attempt
/// (e.g. `CONNECT example.com:443 HTTP/1.1`). Point `HTTP_PROXY`/`HTTPS_PROXY` at it to detect a process reaching
/// beyond loopback (TST-021). Stops when dropped.
#[derive(Debug)]
pub struct DenyProxy {
    addr: SocketAddr,
    attempts: Arc<Mutex<Vec<String>>>,
    task: JoinHandle<()>,
}

impl DenyProxy {
    pub async fn start() -> anyhow::Result<Self> {
        let listener = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0))).await?;
        let addr = listener.local_addr()?;
        let attempts = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&attempts);
        let task = tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                let recorded = Arc::clone(&recorded);
                tokio::spawn(async move {
                    let mut buffer = vec![0_u8; 4096];
                    let read = socket.read(&mut buffer).await.unwrap_or(0);
                    let head = String::from_utf8_lossy(&buffer[..read]);
                    let request_line = head.lines().next().unwrap_or_default().to_owned();
                    recorded
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .push(request_line);
                    let _ = socket
                        .write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                        .await;
                });
            }
        });
        Ok(Self {
            addr,
            attempts,
            task,
        })
    }

    /// Proxy URL for `HTTP_PROXY` / `HTTPS_PROXY`.
    #[must_use]
    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    /// Request lines of every connection attempted so far.
    #[must_use]
    pub fn attempts(&self) -> Vec<String> {
        self.attempts
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Attempts whose target host is not in `allowed` (e.g. [`crate::KNOWN_DISCOVERY_HOSTS`]).
    #[must_use]
    pub fn unexpected_attempts(&self, allowed: &[&str]) -> Vec<String> {
        self.attempts()
            .into_iter()
            .filter(|attempt| !allowed.contains(&target_host(attempt)))
            .collect()
    }
}

/// Host of a proxy request line: `CONNECT host:443 HTTP/1.1` or `GET http://host:80/path HTTP/1.1`.
fn target_host(request_line: &str) -> &str {
    let target = request_line.split_whitespace().nth(1).unwrap_or_default();
    let without_scheme = target.split_once("://").map_or(target, |(_, rest)| rest);
    let authority = without_scheme.split('/').next().unwrap_or_default();
    authority
        .rsplit_once(':')
        .map_or(authority, |(host, _)| host)
}

impl Drop for DenyProxy {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpStream;

    #[test]
    fn target_host_reads_connect_and_absolute_form_requests() {
        assert_eq!(
            target_host("CONNECT catalog.example:443 HTTP/1.1"),
            "catalog.example"
        );
        assert_eq!(
            target_host("GET http://plain.example/models HTTP/1.1"),
            "plain.example"
        );
        assert_eq!(
            target_host("GET http://plain.example:8080/x HTTP/1.1"),
            "plain.example"
        );
        assert_eq!(target_host("garbage"), "");
    }

    #[tokio::test]
    async fn unexpected_attempts_ignore_allowed_hosts() {
        let proxy = DenyProxy::start().await.unwrap();
        for line in [
            "CONNECT allowed.example:443 HTTP/1.1",
            "CONNECT stray.example:443 HTTP/1.1",
        ] {
            let mut stream = TcpStream::connect(proxy.addr).await.unwrap();
            stream
                .write_all(format!("{line}\r\n\r\n").as_bytes())
                .await
                .unwrap();
            let mut sink = Vec::new();
            stream.read_to_end(&mut sink).await.unwrap();
        }

        assert_eq!(
            proxy.unexpected_attempts(&["allowed.example"]),
            ["CONNECT stray.example:443 HTTP/1.1"]
        );
    }

    #[tokio::test]
    async fn refuses_and_records_connect_attempts() {
        let proxy = DenyProxy::start().await.unwrap();

        let mut stream = TcpStream::connect(proxy.addr).await.unwrap();
        stream
            .write_all(b"CONNECT example.com:443 HTTP/1.1\r\nHost: example.com:443\r\n\r\n")
            .await
            .unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).await.unwrap();

        assert!(response.starts_with("HTTP/1.1 403"), "got {response:?}");
        assert_eq!(proxy.attempts(), ["CONNECT example.com:443 HTTP/1.1"]);
    }

    #[tokio::test]
    async fn starts_with_no_attempts() {
        let proxy = DenyProxy::start().await.unwrap();
        assert_eq!(proxy.attempts(), Vec::<String>::new());
        assert!(proxy.url().starts_with("http://127.0.0.1:"));
    }
}
