//! `openagora serve`: the engine as a long-running service. The desktop app and
//! every MCP client (through `openagora mcp`) connect over the local socket.
//! Each connection opens with a hello that says what it is.

use std::io::{BufReader, Read, Write};
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use interprocess::local_socket::Stream;
use interprocess::local_socket::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::gateway::Gateway;
use crate::ipc::{self, read_message};
use crate::jsonrpc::{Incoming, METHOD_NOT_FOUND, Peer};
use crate::mcp::{self, Client};
use crate::paths::{self, Paths};
use crate::state;

/// What the running daemon records about itself.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonInfo {
    pub pid: u32,
    pub address: String,
    pub version: String,
    pub started_at: u64,
}

pub fn info(paths: &Paths) -> Result<Option<DaemonInfo>> {
    let info: Option<DaemonInfo> = state::read(&paths.daemon_file())?;
    // Only trust the record while something answers on the address.
    Ok(info.filter(|_| ipc::connect(paths).is_ok()))
}

pub fn serve(paths: Paths, catalog: Option<PathBuf>) -> Result<()> {
    let listener = ipc::listen(&paths)?;
    let me = DaemonInfo {
        pid: std::process::id(),
        address: ipc::address(&paths),
        version: env!("CARGO_PKG_VERSION").into(),
        started_at: state::now(),
    };
    state::write(&paths.daemon_file(), &me)?;
    eprintln!("openagora: engine listening on {}", me.address);
    // Sunday's MCP config and hook point at this copy of the engine.
    let copy_paths = paths.clone();
    std::thread::spawn(move || {
        if let Err(e) = paths::install_stable_exe(&copy_paths) {
            eprintln!("openagora: refreshing the engine copy: {e:#}");
        }
    });
    let gateway = Arc::new(Gateway::new(paths, catalog));
    for connection in listener.incoming() {
        match connection {
            Ok(stream) => {
                let gateway = Arc::clone(&gateway);
                std::thread::spawn(move || {
                    if let Err(e) = handle(stream, gateway) {
                        eprintln!("openagora: connection ended: {e:#}");
                    }
                });
            }
            Err(e) => eprintln!("openagora: accepting a connection: {e}"),
        }
    }
    Ok(())
}

fn handle(stream: Stream, gateway: Arc<Gateway>) -> Result<()> {
    let (recv, send) = stream.split();
    let mut reader = BufReader::new(recv);
    let hello = read_message(&mut reader)?.context("the client sent nothing")?;
    let role = hello["hello"]["role"].as_str().unwrap_or_default();
    let client = hello["hello"]["client"].as_str().unwrap_or("unknown");
    match role {
        "mcp" => {
            let client = Arc::new(Client::new(client, Peer::new(Box::new(send))));
            gateway.register(&client);
            mcp::serve(reader, client, gateway)
        }
        "desktop" => {
            let peer = Arc::new(Peer::new(Box::new(send)));
            gateway.attach_desktop(Arc::clone(&peer));
            let result = desktop_loop(&mut reader, &peer);
            peer.close();
            gateway.detach_desktop(&peer);
            result
        }
        other => bail!("unknown client role {other:?}"),
    }
}

/// OpenAgora's window: it answers the engine's questions (confirmations) and
/// can ping.
fn desktop_loop(reader: &mut BufReader<impl Read>, peer: &Peer) -> Result<()> {
    while let Some(message) = read_message(reader)? {
        if let Incoming::Request { id, method, .. } = peer.classify(message) {
            match method.as_str() {
                "ping" => peer.respond(&id, json!({}))?,
                _ => {
                    peer.respond_error(&id, METHOD_NOT_FOUND, &format!("unknown method {method}"))?
                }
            }
        }
    }
    Ok(())
}

/// `openagora mcp`: relays an MCP client's stdio to the daemon, starting the
/// daemon if needed. Nothing else is ever written to stdout.
pub fn bridge(paths: &Paths, catalog: Option<&std::path::Path>, client: &str) -> Result<()> {
    let mut stream = ipc::connect_or_start(paths, catalog)?;
    ipc::hello(&mut stream, "mcp", client)?;
    let (mut recv, mut send) = stream.split();
    std::thread::spawn(move || {
        let mut stdout = std::io::stdout();
        let mut buffer = [0u8; 16 * 1024];
        loop {
            match recv.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if stdout
                        .write_all(&buffer[..n])
                        .and_then(|_| stdout.flush())
                        .is_err()
                    {
                        break;
                    }
                }
            }
        }
        // The engine went away: end the MCP session.
        std::process::exit(0);
    });
    std::io::copy(&mut std::io::stdin().lock(), &mut send)?;
    // The MCP client closed its end.
    std::process::exit(0)
}

/// Stops the running daemon, if there is one.
pub fn stop(paths: &Paths) -> Result<bool> {
    let Some(info) = info(paths)? else {
        return Ok(false);
    };
    crate::process::stop(info.pid, std::time::Duration::from_secs(5))?;
    state::remove(&paths.daemon_file())?;
    ipc::remove_address(paths);
    Ok(true)
}
