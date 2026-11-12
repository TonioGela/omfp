//! The command line's protocol client (03-protocol): finds the daemon, opens one WebSocket on `/api/ws`, says
//! `system.hello` as [`CLIENT_NAME`], then sends requests one at a time.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::path::{Path, PathBuf};

use anyhow::{Context as _, bail};
use futures::{SinkExt as _, StreamExt as _};
use omfp_daemon::config::default_data_dir;
use omfp_daemon::paths::DataDir;
use omfp_protocol::method::SystemHello;
use omfp_protocol::{ErrorBody, HelloParams, Method, PROTOCOL_VERSION, Request, ServerFrame};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

/// Client name announced in `system.hello`, recorded as provenance (PRT-033).
pub const CLIENT_NAME: &str = "omfp/cli";

/// Which daemon to talk to: from the command line, else the local daemon of a data directory (DMN-007).
#[derive(Debug, Clone, Default, PartialEq, Eq, clap::Args)]
pub struct DaemonArgs {
    /// The daemon, e.g. `ws://host:7420/api/ws` or `http://host:7420` [default: the local daemon of the data
    /// directory].
    #[arg(long, value_name = "URL")]
    pub daemon: Option<String>,
    /// Access token (DMN-003) [default: the data directory's token file].
    #[arg(long)]
    pub token: Option<String>,
    /// Data directory of the local daemon [default: the daemon's, `$XDG_DATA_HOME/omfp`, else `~/.local/share/omfp`,
    /// on macOS `~/Library/Application Support/omfp`].
    #[arg(long)]
    pub data_dir: Option<PathBuf>,
}

/// Where to connect and with which token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub url: String,
    pub token: String,
}

impl DaemonArgs {
    /// Resolves the target: `--daemon`, else the daemon `daemon.json` names while it runs; `--token`, else the token
    /// file of the data directory. The data directory is only needed (and its default only looked up) when one of the
    /// two is missing.
    pub fn target(&self) -> anyhow::Result<Target> {
        let url = match &self.daemon {
            Some(daemon) => websocket_url(daemon),
            None => format!("ws://{}/api/ws", local_addr(&self.data_dir()?)?),
        };
        Ok(Target {
            url,
            token: self.token()?,
        })
    }

    /// `--data-dir`, else the daemon's default data directory.
    pub fn data_dir(&self) -> anyhow::Result<PathBuf> {
        data_dir_or_default(
            self.data_dir.clone(),
            std::env::var_os("XDG_DATA_HOME").map(PathBuf::from),
            std::env::var_os("HOME").map(PathBuf::from).as_deref(),
        )
    }

    /// `--token`, else the token file of the data directory.
    pub fn token(&self) -> anyhow::Result<String> {
        match &self.token {
            Some(token) => Ok(token.clone()),
            None => read_token(&self.data_dir()?),
        }
    }
}

/// `data_dir` (a `--data-dir`), else the daemon's default data directory for these variables.
pub(crate) fn data_dir_or_default(
    data_dir: Option<PathBuf>,
    xdg_data_home: Option<PathBuf>,
    home: Option<&Path>,
) -> anyhow::Result<PathBuf> {
    data_dir
        .or_else(|| default_data_dir(xdg_data_home, home))
        .context(
            "cannot determine the data directory: pass --data-dir, or set XDG_DATA_HOME or HOME",
        )
}

/// The WebSocket URL of `--daemon`: `http(s)://` becomes `ws(s)://`, a missing scheme means `ws://`, and a URL
/// without a path gets `/api/ws`.
#[must_use]
pub fn websocket_url(daemon: &str) -> String {
    let (scheme, rest) = match daemon.split_once("://") {
        Some(("http" | "ws", rest)) => ("ws", rest),
        Some(("https" | "wss", rest)) => ("wss", rest),
        Some((other, rest)) => (other, rest),
        None => ("ws", daemon),
    };
    match rest.split_once('/') {
        None | Some((_, "")) => format!("{scheme}://{}/api/ws", rest.trim_end_matches('/')),
        Some(_) => format!("{scheme}://{rest}"),
    }
}

/// Where the daemon running for `data_dir` listens (DMN-007); a daemon bound to every interface is reached on
/// loopback.
pub fn local_addr(data_dir: &Path) -> anyhow::Result<SocketAddr> {
    let info = omfp_daemon::discover(data_dir)
        .with_context(|| format!("cannot read daemon.json in {}", data_dir.display()))?
        .with_context(|| {
            format!(
                "no daemon is running for {}: start one with `omfp daemon`, or pass --daemon <url>",
                data_dir.display()
            )
        })?;
    Ok(reachable(info.addr))
}

pub(crate) fn reachable(addr: SocketAddr) -> SocketAddr {
    match addr.ip() {
        IpAddr::V4(ip) if ip.is_unspecified() => {
            SocketAddr::new(Ipv4Addr::LOCALHOST.into(), addr.port())
        }
        IpAddr::V6(ip) if ip.is_unspecified() => {
            SocketAddr::new(Ipv6Addr::LOCALHOST.into(), addr.port())
        }
        _ => addr,
    }
}

/// The owner's token from `<data-dir>/token` (DMN-003); never created here.
pub(crate) fn read_token(data_dir: &Path) -> anyhow::Result<String> {
    let path = DataDir::new(data_dir).token();
    let token = std::fs::read_to_string(&path).with_context(|| {
        format!(
            "cannot read the token file {} (pass --token)",
            path.display()
        )
    })?;
    let token = token.trim();
    if token.is_empty() {
        bail!("the token file {} is empty (pass --token)", path.display());
    }
    Ok(token.to_owned())
}

/// An authenticated connection to a daemon.
#[derive(Debug)]
pub struct Connection {
    ws: WebSocketStream<MaybeTlsStream<TcpStream>>,
    next_id: u64,
}

impl Connection {
    /// Connects to `target` and says `system.hello` (PRT-001).
    pub async fn open(target: &Target) -> anyhow::Result<Self> {
        let (ws, _) = connect_async(target.url.as_str())
            .await
            .with_context(|| format!("cannot connect to the daemon at {}", target.url))?;
        let mut connection = Self { ws, next_id: 0 };
        connection
            .call::<SystemHello>(HelloParams {
                token: Some(target.token.clone()),
                ticket: None,
                protocol: PROTOCOL_VERSION,
                client: CLIENT_NAME.to_owned(),
            })
            .await?;
        Ok(connection)
    }

    /// Sends one request and waits for its response; event frames in between are skipped (nothing is subscribed).
    pub async fn call<M: Method>(&mut self, params: M::Params) -> anyhow::Result<M::Result> {
        self.next_id += 1;
        let id = format!("cli-{}", self.next_id);
        let request = serde_json::to_string(&Request::new(id.clone(), M::call(params)))?;
        self.ws
            .send(Message::Text(request.into()))
            .await
            .with_context(|| format!("{}: cannot send the request", M::NAME))?;
        loop {
            let message = self
                .ws
                .next()
                .await
                .with_context(|| format!("{}: the daemon closed the connection", M::NAME))?
                .with_context(|| format!("{}: the connection failed", M::NAME))?;
            let text = match message {
                Message::Text(text) => text,
                Message::Close(frame) => {
                    let reason = frame.map(|frame| format!(" ({}: {})", frame.code, frame.reason));
                    bail!(
                        "{}: the daemon closed the connection{}",
                        M::NAME,
                        reason.unwrap_or_default()
                    );
                }
                _ => continue,
            };
            let frame: ServerFrame = serde_json::from_str(text.as_str())
                .with_context(|| format!("{}: unexpected frame from the daemon", M::NAME))?;
            if let ServerFrame::Response(response) = frame
                && response.id == id
            {
                let result = response.result.map_err(|error| failure(M::NAME, &error))?;
                return serde_json::from_value(result)
                    .with_context(|| format!("{}: unexpected result from the daemon", M::NAME));
            }
        }
    }

    /// Closes the connection cleanly.
    pub async fn close(mut self) -> anyhow::Result<()> {
        self.ws
            .close(None)
            .await
            .context("cannot close the connection")
    }
}

/// `<method> failed: <message> (<code>)`, with the wire name of the error code (PRT-011).
fn failure(method: &str, error: &ErrorBody) -> anyhow::Error {
    let code = serde_json::to_value(error.code)
        .ok()
        .and_then(|code| code.as_str().map(str::to_owned))
        .unwrap_or_default();
    anyhow::anyhow!("{method} failed: {} ({code})", error.message)
}

#[cfg(test)]
mod tests {
    use super::*;

    // spec: UI-091
    #[test]
    fn daemon_urls_become_websocket_urls_on_api_ws() {
        for (given, expected) in [
            ("ws://127.0.0.1:7420/api/ws", "ws://127.0.0.1:7420/api/ws"),
            ("http://127.0.0.1:7420", "ws://127.0.0.1:7420/api/ws"),
            ("http://127.0.0.1:7420/", "ws://127.0.0.1:7420/api/ws"),
            ("https://omfp.example.com", "wss://omfp.example.com/api/ws"),
            (
                "wss://omfp.example.com/proxy/api/ws",
                "wss://omfp.example.com/proxy/api/ws",
            ),
            ("localhost:7420", "ws://localhost:7420/api/ws"),
        ] {
            assert_eq!(websocket_url(given), expected, "{given}");
        }
    }

    #[test]
    fn a_daemon_on_every_interface_is_reached_on_loopback() {
        assert_eq!(
            reachable("0.0.0.0:7420".parse().unwrap()),
            "127.0.0.1:7420".parse().unwrap()
        );
        assert_eq!(
            reachable("[::]:7420".parse().unwrap()),
            "[::1]:7420".parse().unwrap()
        );
        assert_eq!(
            reachable("192.168.1.2:7420".parse().unwrap()),
            "192.168.1.2:7420".parse().unwrap()
        );
    }

    // spec: UI-091, DMN-003
    #[test]
    fn the_token_comes_from_the_data_dir_unless_given() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("token"), "abc\n").unwrap();
        let args = DaemonArgs {
            daemon: Some("http://127.0.0.1:1".into()),
            token: None,
            data_dir: Some(dir.path().into()),
        };
        assert_eq!(args.target().unwrap().token, "abc");

        let given = DaemonArgs {
            token: Some("given".into()),
            ..args
        };
        assert_eq!(given.target().unwrap().token, "given");
    }
}
