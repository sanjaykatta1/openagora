//! OpenAgora's connections to apps that protect their MCP server with OAuth
//! (T3 Code). The grant is the engine's own: the user approves OpenAgora on
//! the app's own page, and the engine keeps it in
//! `<data>/connections/<app>.toml`, which only this user can open. Grants
//! that can't be refreshed are asked for again once they expire.

use std::path::PathBuf;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::paths::Paths;
use crate::state;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Connection {
    pub app: String,
    /// The client id the app gave OpenAgora when it registered.
    pub client_id: String,
    #[serde(default)]
    pub access_token: String,
    /// Seconds since the Unix epoch; 0 when the app didn't say.
    #[serde(default)]
    pub expires_at: u64,
    pub connected_at: u64,
    /// The port the app ran on when the user approved: the grant names it.
    pub port: u16,
    /// What the user allowed on the app's page, in the app's words.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    /// The app refused the grant since: it expired or was taken back.
    #[serde(default)]
    pub signed_out: bool,
}

impl Connection {
    /// Whether the grant can still be used, as far as OpenAgora knows.
    pub fn usable(&self) -> bool {
        !self.access_token.is_empty()
            && !self.signed_out
            && (self.expires_at == 0 || self.expires_at > state::now())
    }

    /// Whether the user allowed more than reading (T3 Code: Supervised or
    /// higher), which starting tasks needs.
    pub fn can_act(&self) -> bool {
        self.scope
            .as_deref()
            .is_none_or(|scope| scope.split_whitespace().any(|s| s.ends_with(":operate")))
    }
}

fn dir(paths: &Paths) -> PathBuf {
    paths.root.join("connections")
}

fn file(paths: &Paths, app: &str) -> PathBuf {
    dir(paths).join(format!("{app}.toml"))
}

pub fn get(paths: &Paths, app: &str) -> Result<Option<Connection>> {
    if !crate::manifest::valid_id(app) {
        return Ok(None);
    }
    state::read(&file(paths, app))
}

pub fn save(paths: &Paths, connection: &Connection) -> Result<()> {
    state::write_private(&file(paths, &connection.app), connection)
}

/// The app refused the grant: forget the token, and ask the user again.
pub fn sign_out(paths: &Paths, app: &str) -> Result<()> {
    if let Some(mut connection) = get(paths, app)? {
        connection.access_token.clear();
        connection.signed_out = true;
        save(paths, &connection)?;
    }
    Ok(())
}

/// Forgets the grant. Returns whether there was one.
pub fn remove(paths: &Paths, app: &str) -> Result<bool> {
    let path = file(paths, app);
    let existed = path.exists();
    state::remove(&path)?;
    Ok(existed)
}

pub fn list(paths: &Paths) -> Result<Vec<Connection>> {
    let Ok(entries) = std::fs::read_dir(dir(paths)) else {
        return Ok(Vec::new());
    };
    let mut all: Vec<Connection> = entries
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "toml"))
        .filter_map(|e| state::read::<Connection>(&e.path()).ok().flatten())
        .collect();
    all.sort_by(|a, b| a.app.cmp(&b.app));
    Ok(all)
}
