//! The desk: what the user and their agents put down to share (screenshots,
//! text, links, files), kept on this computer and cleared after 24 hours.
//! Each payload is stored once, named by its SHA-256, and items point to it.
//!
//! ```text
//! <data>/desk/items/<id>.json  an item: what it is, where it came from, who may read it
//! <data>/desk/blobs/<sha256>   its payload
//! ```

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::github::sha256_hex;
use crate::paths::Paths;
use crate::state;

/// How long items stay on the desk.
pub const KEEP_FOR: u64 = 24 * 60 * 60;
/// The largest payload the desk takes.
pub const MAX_BYTES: usize = 20 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// A capture of a window.
    Screenshot,
    /// Any other picture, e.g. from the clipboard.
    Image,
    Text,
    Link,
    File,
}

impl Kind {
    pub fn parse(text: &str) -> Option<Kind> {
        match text {
            "screenshot" => Some(Kind::Screenshot),
            "image" => Some(Kind::Image),
            "text" => Some(Kind::Text),
            "link" => Some(Kind::Link),
            "file" => Some(Kind::File),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Screenshot => "screenshot",
            Kind::Image => "image",
            Kind::Text => "text",
            Kind::Link => "link",
            Kind::File => "file",
        }
    }
}

/// Where an item came from.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Source {
    /// The app's name, or "Clipboard", "Files", …
    pub app: String,
    /// The app's key (bundle id, executable, window class, `openagora:<id>`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    pub id: String,
    pub kind: Kind,
    /// A MIME type, e.g. `image/png`.
    #[serde(rename = "type")]
    pub mime: String,
    pub source: Source,
    pub created_at: u64,
    pub expires_at: u64,
    /// The clients that may read it, e.g. `["sunday"]`.
    pub shared_with: Vec<String>,
    /// Who put it here: `user`, or a client.
    pub added_by: String,
    /// The payload's SHA-256.
    pub blob: String,
    pub size: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    /// A file's name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// The family of a MIME type: `image/png` is in `image/*`.
pub fn family(mime: &str) -> String {
    match mime.split_once('/') {
        Some((top, _)) if !top.is_empty() => format!("{top}/*"),
        _ => "application/*".into(),
    }
}

impl Item {
    pub fn family(&self) -> String {
        family(&self.mime)
    }

    pub fn is_shared_with(&self, client: &str) -> bool {
        self.shared_with.iter().any(|c| c == client)
    }

    /// A short description for people and agents, e.g.
    /// `screenshot of Firefox, window "Docs", 1280×800`.
    pub fn describe(&self) -> String {
        let mut text = match self.kind {
            Kind::Screenshot => format!("screenshot of {}", self.source.app),
            Kind::Image => format!("image from {}", self.source.app),
            Kind::Text => format!("text from {} ({} bytes)", self.source.app, self.size),
            Kind::Link => format!("link from {}", self.source.app),
            Kind::File => format!(
                "file {} from {} ({}, {} bytes)",
                self.name.as_deref().unwrap_or("(unnamed)"),
                self.source.app,
                self.mime,
                self.size
            ),
        };
        if let Some(title) = &self.source.title {
            text.push_str(&format!(", window \u{201c}{title}\u{201d}"));
        }
        if let (Some(w), Some(h)) = (self.width, self.height) {
            text.push_str(&format!(", {w}\u{d7}{h}"));
        }
        text
    }
}

/// Something to put on the desk.
#[derive(Debug, Clone)]
pub struct New {
    pub kind: Kind,
    pub mime: String,
    pub source: Source,
    pub shared_with: Vec<String>,
    pub added_by: String,
    pub bytes: Vec<u8>,
    pub name: Option<String>,
}

fn dir(paths: &Paths) -> PathBuf {
    paths.root.join("desk")
}

fn item_file(paths: &Paths, id: &str) -> PathBuf {
    dir(paths).join("items").join(format!("{id}.json"))
}

fn blob_file(paths: &Paths, blob: &str) -> PathBuf {
    dir(paths).join("blobs").join(blob)
}

/// Desk ids are `d-` and ten hex digits.
fn valid_id(id: &str) -> bool {
    id.strip_prefix("d-")
        .is_some_and(|hex| hex.len() == 10 && hex.chars().all(|c| c.is_ascii_hexdigit()))
}

fn ensure_dirs(paths: &Paths) -> Result<()> {
    let root = dir(paths);
    for sub in ["items", "blobs"] {
        fs::create_dir_all(root.join(sub))
            .with_context(|| format!("creating {}", root.join(sub).display()))?;
    }
    // Screenshots are private: only this user may look in the desk.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

/// The size of a PNG, GIF or JPEG picture, read from its header.
pub fn image_size(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") && bytes.len() >= 24 {
        let w = u32::from_be_bytes(bytes[16..20].try_into().ok()?);
        let h = u32::from_be_bytes(bytes[20..24].try_into().ok()?);
        return Some((w, h));
    }
    if bytes.starts_with(b"GIF8") && bytes.len() >= 10 {
        let w = u16::from_le_bytes([bytes[6], bytes[7]]) as u32;
        let h = u16::from_le_bytes([bytes[8], bytes[9]]) as u32;
        return Some((w, h));
    }
    if bytes.starts_with(&[0xff, 0xd8]) {
        // Walk the JPEG segments to the first start-of-frame.
        let mut i = 2;
        while i + 9 < bytes.len() {
            if bytes[i] != 0xff {
                return None;
            }
            let marker = bytes[i + 1];
            let len = u16::from_be_bytes([bytes[i + 2], bytes[i + 3]]) as usize;
            let frame = matches!(marker, 0xc0..=0xcf) && !matches!(marker, 0xc4 | 0xc8 | 0xcc);
            if frame {
                let h = u16::from_be_bytes([bytes[i + 5], bytes[i + 6]]) as u32;
                let w = u16::from_be_bytes([bytes[i + 7], bytes[i + 8]]) as u32;
                return Some((w, h));
            }
            i += 2 + len;
        }
    }
    None
}

/// The picture types models take.
pub fn image_type(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("image/jpeg")
    } else if bytes.starts_with(b"GIF8") {
        Some("image/gif")
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

static NEXT: AtomicU64 = AtomicU64::new(0);

/// Puts something on the desk.
pub fn add(paths: &Paths, new: New) -> Result<Item> {
    if new.bytes.is_empty() {
        bail!("there is nothing to put on the desk");
    }
    if new.bytes.len() > MAX_BYTES {
        bail!(
            "that is too big for the desk ({} MB; at most {} MB)",
            new.bytes.len() / (1024 * 1024),
            MAX_BYTES / (1024 * 1024)
        );
    }
    let mime = match new.kind {
        Kind::Screenshot | Kind::Image => match image_type(&new.bytes) {
            Some(mime) => mime.to_string(),
            None => bail!("that isn't a PNG, JPEG, GIF or WebP picture"),
        },
        Kind::Text | Kind::Link => {
            if std::str::from_utf8(&new.bytes).is_err() {
                bail!("text on the desk must be UTF-8");
            }
            new.mime.clone()
        }
        Kind::File => new.mime.clone(),
    };
    ensure_dirs(paths)?;
    sweep(paths)?;
    let blob = sha256_hex(&new.bytes);
    let blob_path = blob_file(paths, &blob);
    if !blob_path.exists() {
        let partial = blob_path.with_extension("partial");
        fs::write(&partial, &new.bytes)
            .with_context(|| format!("writing {}", partial.display()))?;
        fs::rename(&partial, &blob_path)?;
    }
    let now = state::now();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let seed = format!(
        "{blob}:{nanos}:{}:{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::SeqCst)
    );
    let id = format!("d-{}", &sha256_hex(seed.as_bytes())[..10]);
    let (width, height) = match new.kind {
        Kind::Screenshot | Kind::Image => image_size(&new.bytes).unzip(),
        _ => (None, None),
    };
    let item = Item {
        id,
        kind: new.kind,
        mime,
        source: new.source,
        created_at: now,
        expires_at: now + KEEP_FOR,
        shared_with: new.shared_with,
        added_by: new.added_by,
        blob,
        size: new.bytes.len() as u64,
        width,
        height,
        name: new.name,
    };
    let file = item_file(paths, &item.id);
    let partial = file.with_extension("json.partial");
    fs::write(&partial, serde_json::to_vec_pretty(&item)?)
        .with_context(|| format!("writing {}", partial.display()))?;
    fs::rename(&partial, &file)?;
    Ok(item)
}

fn read_item(path: &std::path::Path) -> Option<Item> {
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}

/// Every item still on the desk, newest first.
pub fn list(paths: &Paths) -> Result<Vec<Item>> {
    sweep(paths)?;
    let Ok(entries) = fs::read_dir(dir(paths).join("items")) else {
        return Ok(Vec::new());
    };
    let mut items: Vec<Item> = entries
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| read_item(&e.path()))
        .collect();
    items.sort_by(|a, b| {
        b.created_at
            .cmp(&a.created_at)
            .then_with(|| b.id.cmp(&a.id))
    });
    Ok(items)
}

/// One item, unless it expired or never existed.
pub fn get(paths: &Paths, id: &str) -> Result<Option<Item>> {
    if !valid_id(id) {
        return Ok(None);
    }
    Ok(read_item(&item_file(paths, id)).filter(|item| item.expires_at > state::now()))
}

/// An item's payload.
pub fn payload(paths: &Paths, item: &Item) -> Result<Vec<u8>> {
    let path = blob_file(paths, &item.blob);
    fs::read(&path).with_context(|| format!("reading {}", path.display()))
}

/// Takes one item off the desk.
pub fn remove(paths: &Paths, id: &str) -> Result<bool> {
    if !valid_id(id) {
        return Ok(false);
    }
    let removed = fs::remove_file(item_file(paths, id)).is_ok();
    sweep(paths)?;
    Ok(removed)
}

/// Clears the desk. Returns how many items went.
pub fn clear(paths: &Paths) -> Result<usize> {
    let count = list(paths)?.len();
    let root = dir(paths);
    if root.exists() {
        fs::remove_dir_all(&root).with_context(|| format!("clearing {}", root.display()))?;
    }
    Ok(count)
}

/// Removes expired items, and payloads no item points to any more.
pub fn sweep(paths: &Paths) -> Result<()> {
    let items_dir = dir(paths).join("items");
    let Ok(entries) = fs::read_dir(&items_dir) else {
        return Ok(());
    };
    let now = state::now();
    let mut kept = std::collections::BTreeSet::new();
    for entry in entries.flatten() {
        let path = entry.path();
        match read_item(&path) {
            Some(item) if item.expires_at > now => {
                kept.insert(item.blob);
            }
            // Expired, or left half-written: gone.
            _ => {
                let fresh = entry
                    .metadata()
                    .and_then(|m| m.modified())
                    .is_ok_and(|m| m.elapsed().is_ok_and(|age| age.as_secs() < 60));
                if !(fresh && path.extension().is_some_and(|x| x == "partial")) {
                    let _ = fs::remove_file(&path);
                }
            }
        }
    }
    if let Ok(blobs) = fs::read_dir(dir(paths).join("blobs")) {
        for blob in blobs.flatten() {
            let name = blob.file_name().to_string_lossy().into_owned();
            let fresh = blob
                .metadata()
                .and_then(|m| m.modified())
                .is_ok_and(|m| m.elapsed().is_ok_and(|age| age.as_secs() < 60));
            if !kept.contains(&name) && !fresh {
                let _ = fs::remove_file(blob.path());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 2×3 PNG header (enough for image_size and image_type).
    pub fn png(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        bytes.extend_from_slice(&width.to_be_bytes());
        bytes.extend_from_slice(&height.to_be_bytes());
        bytes.extend_from_slice(&[8, 6, 0, 0, 0]);
        bytes
    }

    fn screenshot(paths: &Paths, bytes: Vec<u8>) -> Item {
        add(
            paths,
            New {
                kind: Kind::Screenshot,
                mime: String::new(),
                source: Source {
                    app: "Firefox".into(),
                    key: Some("firefox".into()),
                    title: Some("Docs".into()),
                    url: None,
                },
                shared_with: vec!["sunday".into()],
                added_by: "user".into(),
                bytes,
                name: None,
            },
        )
        .unwrap()
    }

    #[test]
    fn keeps_items_and_payloads_once() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths {
            root: dir.path().to_path_buf(),
        };
        let first = screenshot(&paths, png(1280, 800));
        let second = screenshot(&paths, png(1280, 800));
        assert_ne!(first.id, second.id);
        assert_eq!(first.blob, second.blob);
        assert_eq!(first.mime, "image/png");
        assert_eq!(first.family(), "image/*");
        assert_eq!((first.width, first.height), (Some(1280), Some(800)));
        assert!(first.is_shared_with("sunday") && !first.is_shared_with("codex"));
        assert_eq!(
            first.describe(),
            "screenshot of Firefox, window \u{201c}Docs\u{201d}, 1280\u{d7}800"
        );
        assert_eq!(
            fs::read_dir(super::dir(&paths).join("blobs"))
                .unwrap()
                .count(),
            1
        );
        assert_eq!(list(&paths).unwrap().len(), 2);
        assert_eq!(payload(&paths, &first).unwrap(), png(1280, 800));
        assert_eq!(get(&paths, &first.id).unwrap().unwrap(), first);
        assert_eq!(get(&paths, "../../etc/passwd").unwrap(), None);

        // Pictures must be pictures; text must be text.
        let mut not_png = screenshot_new();
        not_png.bytes = b"<html>".to_vec();
        assert!(add(&paths, not_png).is_err());

        // Taking one off keeps the shared payload; expiry clears the rest.
        assert!(remove(&paths, &first.id).unwrap());
        assert_eq!(payload(&paths, &second).unwrap(), png(1280, 800));
        let mut expired = second.clone();
        expired.expires_at = state::now() - 1;
        fs::write(
            item_file(&paths, &second.id),
            serde_json::to_vec(&expired).unwrap(),
        )
        .unwrap();
        // The payload is fresh, so it waits a minute before it goes.
        assert!(list(&paths).unwrap().is_empty());
        assert_eq!(get(&paths, &second.id).unwrap(), None);
        assert_eq!(clear(&paths).unwrap(), 0);
        assert!(!super::dir(&paths).exists());
    }

    fn screenshot_new() -> New {
        New {
            kind: Kind::Screenshot,
            mime: String::new(),
            source: Source::default(),
            shared_with: vec![],
            added_by: "user".into(),
            bytes: vec![],
            name: None,
        }
    }

    #[test]
    fn reads_picture_sizes() {
        assert_eq!(image_size(&png(2, 3)), Some((2, 3)));
        assert_eq!(image_type(&png(2, 3)), Some("image/png"));
        // A minimal JPEG: SOI, an APP0 segment, then SOF0 with 640×480.
        let mut jpeg = vec![0xff, 0xd8, 0xff, 0xe0, 0x00, 0x04, 0x00, 0x00];
        jpeg.extend_from_slice(&[0xff, 0xc0, 0x00, 0x0b, 0x08, 0x01, 0xe0, 0x02, 0x80, 0x01]);
        assert_eq!(image_size(&jpeg), Some((640, 480)));
        assert_eq!(image_type(&jpeg), Some("image/jpeg"));
        assert_eq!(image_type(b"GIF89a\x01\0\x01\0"), Some("image/gif"));
        assert_eq!(image_size(b"GIF89a\x01\0\x02\0"), Some((1, 2)));
        assert_eq!(image_type(b"hello"), None);
        assert_eq!(family("text/plain"), "text/*");
        assert_eq!(family("nonsense"), "application/*");
    }
}
