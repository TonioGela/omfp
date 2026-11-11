//! `omfp mcp config` (DMN-159, AGT-054): the `mcp.json` document that points an MCP client (omp outside omfp, any
//! other agent) at a daemon's MCP server (DMN-155) with its token (DMN-158).

use serde_json::{Value, json};

use crate::remote::{DaemonArgs, local_addr};

/// `omfp mcp …`.
#[derive(Debug, Clone, PartialEq, Eq, clap::Subcommand)]
pub enum Command {
    /// Prints the `mcp.json` entry of the daemon's MCP server: put it in `~/.omp/agent/mcp.json` (every omp session)
    /// or a project's `.omp/mcp.json`. It holds the daemon's token: keep the file private.
    Config(ConfigArgs),
}

/// `omfp mcp config`.
#[derive(Debug, Clone, PartialEq, Eq, clap::Args)]
pub struct ConfigArgs {
    #[command(flatten)]
    pub daemon: DaemonArgs,
    /// The server's name in `mcp.json`; omp names its tools `mcp__<name>_…`.
    #[arg(long, default_value = "omfp")]
    pub name: String,
}

/// Runs `omfp mcp …`; returns what to print.
pub fn run(command: &Command) -> anyhow::Result<String> {
    match command {
        Command::Config(args) => config(args),
    }
}

fn config(args: &ConfigArgs) -> anyhow::Result<String> {
    let url = match &args.daemon.daemon {
        Some(daemon) => mcp_url(daemon)?,
        None => format!("http://{}/mcp", local_addr(&args.daemon.data_dir()?)?),
    };
    let token = args.daemon.token()?;
    Ok(serde_json::to_string_pretty(&document(
        &args.name, &url, &token,
    ))?)
}

/// `{ "mcpServers": { <name>: … } }`: Streamable HTTP at `url`, the token as bearer, and no request timeout, so
/// `omfp_thread_wait` may outlast omp's 30 s default (DMN-159).
#[must_use]
pub fn document(name: &str, url: &str, token: &str) -> Value {
    json!({
        "mcpServers": {
            name: {
                "type": "http",
                "url": url,
                "headers": { "Authorization": format!("Bearer {token}") },
                "timeout": 0,
            }
        }
    })
}

/// The MCP endpoint of `--daemon`: `http(s)://` or `ws(s)://` (a missing scheme means `http://`), its path ignored.
pub fn mcp_url(daemon: &str) -> anyhow::Result<String> {
    let (scheme, rest) = match daemon.split_once("://") {
        Some(("http" | "ws", rest)) => ("http", rest),
        Some(("https" | "wss", rest)) => ("https", rest),
        Some((other, _)) => {
            anyhow::bail!("--daemon `{daemon}`: the scheme `{other}` is not http(s):// or ws(s)://")
        }
        None => ("http", daemon),
    };
    let authority = rest.split('/').next().unwrap_or_default();
    anyhow::ensure!(!authority.is_empty(), "--daemon `{daemon}` names no host");
    Ok(format!("{scheme}://{authority}/mcp"))
}

#[cfg(test)]
mod tests {
    use super::*;

    // spec: DMN-159
    #[test]
    fn the_daemon_url_becomes_its_mcp_endpoint() {
        for (given, expected) in [
            ("http://127.0.0.1:7420", "http://127.0.0.1:7420/mcp"),
            ("http://127.0.0.1:7420/", "http://127.0.0.1:7420/mcp"),
            ("ws://127.0.0.1:7420/api/ws", "http://127.0.0.1:7420/mcp"),
            (
                "https://omfp.example.ts.net",
                "https://omfp.example.ts.net/mcp",
            ),
            (
                "wss://omfp.example.ts.net/api/ws",
                "https://omfp.example.ts.net/mcp",
            ),
            ("localhost:7420", "http://localhost:7420/mcp"),
        ] {
            assert_eq!(mcp_url(given).unwrap(), expected, "{given}");
        }
        for wrong in ["ftp://host", "http://", ""] {
            assert!(mcp_url(wrong).is_err(), "{wrong}");
        }
    }

    // spec: DMN-159, AGT-054
    #[test]
    fn the_document_names_the_endpoint_the_token_and_no_timeout() {
        assert_eq!(
            document("omfp", "http://127.0.0.1:7420/mcp", "abc"),
            json!({ "mcpServers": { "omfp": {
                "type": "http",
                "url": "http://127.0.0.1:7420/mcp",
                "headers": { "Authorization": "Bearer abc" },
                "timeout": 0,
            } } })
        );
    }

    // spec: DMN-159
    #[test]
    fn the_token_comes_from_the_data_dir_unless_given() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("token"), "abc\n").unwrap();
        let args = ConfigArgs {
            daemon: DaemonArgs {
                daemon: Some("http://127.0.0.1:1".into()),
                token: None,
                data_dir: Some(dir.path().into()),
            },
            name: "work".into(),
        };
        let printed: Value = serde_json::from_str(&config(&args).unwrap()).unwrap();
        assert_eq!(
            printed["mcpServers"]["work"]["headers"]["Authorization"],
            "Bearer abc"
        );
        assert_eq!(
            printed["mcpServers"]["work"]["url"],
            "http://127.0.0.1:1/mcp"
        );

        let no_daemon = ConfigArgs {
            daemon: DaemonArgs {
                daemon: None,
                ..args.daemon
            },
            ..args
        };
        let error = config(&no_daemon).unwrap_err().to_string();
        assert!(error.contains("no daemon is running for"), "{error}");
    }
}
