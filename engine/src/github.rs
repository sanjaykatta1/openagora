//! Fetches release assets from GitHub. Uses the system `curl` (present on macOS,
//! Linux and Windows 10+), which also honours the user's proxy settings.

use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone)]
pub struct Asset {
    pub name: String,
    pub url: String,
    /// `sha256:<hex>` when GitHub reports one.
    pub digest: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Release {
    pub tag: String,
    pub assets: Vec<Asset>,
}

/// `OPENAGORA_GITHUB_API` points at another API root (tests, mirrors).
fn api_root() -> String {
    env::var("OPENAGORA_GITHUB_API").unwrap_or_else(|_| "https://api.github.com".into())
}

pub fn latest_release(repo: &str) -> Result<Release> {
    let url = format!("{}/repos/{repo}/releases/latest", api_root());
    let mut args = vec![
        "-H".to_string(),
        "Accept: application/vnd.github+json".to_string(),
    ];
    // A token raises the rate limit; curl does not forward it on redirects.
    if let Ok(token) = env::var("GITHUB_TOKEN")
        && !token.is_empty()
    {
        args.push("-H".into());
        args.push(format!("Authorization: Bearer {token}"));
    }
    args.push(url.clone());
    let body = curl(&args).with_context(|| format!("looking up the latest release of {repo}"))?;
    let json: serde_json::Value =
        serde_json::from_slice(&body).with_context(|| format!("reading {url}"))?;
    let assets = json["assets"]
        .as_array()
        .context("release has no asset list")?
        .iter()
        .filter_map(|a| {
            Some(Asset {
                name: a["name"].as_str()?.to_string(),
                url: a["browser_download_url"].as_str()?.to_string(),
                digest: a["digest"].as_str().map(str::to_string),
            })
        })
        .collect();
    Ok(Release {
        tag: json["tag_name"].as_str().unwrap_or("unknown").to_string(),
        assets,
    })
}

/// The single asset matching `pattern`, where `*` matches any run of characters.
pub fn pick<'a>(release: &'a Release, pattern: &str) -> Result<&'a Asset> {
    let matches: Vec<&Asset> = release
        .assets
        .iter()
        .filter(|a| glob(pattern, &a.name))
        .collect();
    match matches.as_slice() {
        [one] => Ok(one),
        [] => bail!("release {} has no asset matching {pattern:?}", release.tag),
        many => bail!(
            "release {} has {} assets matching {pattern:?}; the listing must pick one",
            release.tag,
            many.len()
        ),
    }
}

/// Downloads `asset` to `dest`, verifying its SHA-256 when GitHub reports one.
pub fn download(asset: &Asset, dest: &Path) -> Result<()> {
    curl(&[
        "-o".to_string(),
        dest.display().to_string(),
        asset.url.clone(),
    ])
    .with_context(|| format!("downloading {}", asset.name))?;
    if let Some(expected) = asset
        .digest
        .as_deref()
        .and_then(|d| d.strip_prefix("sha256:"))
    {
        let actual = sha256_hex(&fs::read(dest)?);
        if !actual.eq_ignore_ascii_case(expected) {
            let _ = fs::remove_file(dest);
            bail!("{} failed its checksum check; it was deleted", asset.name);
        }
    }
    Ok(())
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn curl(args: &[String]) -> Result<Vec<u8>> {
    let out = Command::new("curl")
        .args(["-fsSL", "--retry", "2"])
        .args(args)
        .output()
        .context("running curl (is it installed?)")?;
    if !out.status.success() {
        bail!("{}", String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(out.stdout)
}

pub fn glob(pattern: &str, name: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return pattern == name;
    }
    let (first, last) = (parts[0], parts[parts.len() - 1]);
    if !name.starts_with(first) || name.len() < first.len() + last.len() || !name.ends_with(last) {
        return false;
    }
    let mut rest = &name[first.len()..name.len() - last.len()];
    for part in &parts[1..parts.len() - 1] {
        match rest.find(part) {
            Some(i) => rest = &rest[i + part.len()..],
            None => return false,
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::glob;

    #[test]
    fn glob_matches() {
        assert!(glob("Handy_*_amd64.AppImage", "Handy_0.9.8_amd64.AppImage"));
        assert!(!glob(
            "Handy_*_amd64.AppImage",
            "Handy_0.9.8_amd64.AppImage.sig"
        ));
        assert!(!glob(
            "Handy_*_amd64.AppImage",
            "Handy_0.9.8_aarch64.AppImage"
        ));
        assert!(glob("Maccy.app.zip", "Maccy.app.zip"));
        assert!(!glob("Maccy.app.zip", "Maccy.app.zip.sig"));
        assert!(glob("a*b*c", "a-x-b-y-c"));
        assert!(!glob("ab*ba", "aba"));
    }
}
