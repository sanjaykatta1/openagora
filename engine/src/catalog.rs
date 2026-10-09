//! A catalog is a folder of listings: `<root>/apps/<id>/app.toml`.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::manifest::Manifest;

#[derive(Debug)]
pub struct Listing {
    pub path: PathBuf,
    pub manifest: Manifest,
}

#[derive(Debug, Default)]
pub struct Catalog {
    pub listings: BTreeMap<String, Listing>,
}

mod embedded {
    include!(concat!(env!("OUT_DIR"), "/embedded_catalog.rs"));
}

impl Catalog {
    /// The catalog snapshot built into this binary.
    pub fn embedded() -> Result<Catalog> {
        Catalog::collect(embedded::EMBEDDED.iter().map(|(folder, text)| {
            parse_listing(
                PathBuf::from(format!("<built-in>/{folder}/app.toml")),
                folder,
                text,
            )
        }))
    }

    /// Loads every listing under `<root>/apps`.
    pub fn load(root: &Path) -> Result<Catalog> {
        let apps = root.join("apps");
        let mut entries: Vec<PathBuf> = fs::read_dir(&apps)
            .with_context(|| format!("reading {}", apps.display()))?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.is_dir())
            .collect();
        entries.sort();
        Catalog::collect(entries.iter().map(|dir| load_listing(dir)))
    }

    /// Reports every problem at once rather than stopping at the first.
    fn collect(listings: impl Iterator<Item = Result<Listing>>) -> Result<Catalog> {
        let mut catalog = Catalog::default();
        let mut errors = Vec::new();
        for listing in listings {
            match listing {
                Ok(listing) => {
                    let id = listing.manifest.id.clone();
                    if let Some(existing) = catalog.listings.get(&id) {
                        errors.push(format!(
                            "{}: id {id:?} already used by {}",
                            listing.path.display(),
                            existing.path.display()
                        ));
                    } else {
                        catalog.listings.insert(id, listing);
                    }
                }
                Err(e) => errors.push(format!("{e:#}")),
            }
        }
        if !errors.is_empty() {
            bail!("{}", errors.join("\n"));
        }
        Ok(catalog)
    }

    pub fn search(&self, query: &str) -> Vec<&Listing> {
        let q = query.to_lowercase();
        self.listings
            .values()
            .filter(|l| {
                let m = &l.manifest;
                [&m.id, &m.name, &m.summary, &m.category]
                    .iter()
                    .any(|field| field.to_lowercase().contains(&q))
            })
            .collect()
    }
}

pub fn load_listing(dir: &Path) -> Result<Listing> {
    let path = dir.join("app.toml");
    let text = fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let folder = dir.file_name().and_then(|n| n.to_str()).unwrap_or_default();
    parse_listing(path.clone(), folder, &text)
}

fn parse_listing(path: PathBuf, folder: &str, text: &str) -> Result<Listing> {
    let manifest =
        Manifest::parse(text).map_err(|e| anyhow::anyhow!("{}:\n{e}", path.display()))?;
    if folder != manifest.id {
        bail!(
            "{}: id {:?} must match its folder {folder:?}",
            path.display(),
            manifest.id
        );
    }
    Ok(Listing { path, manifest })
}
