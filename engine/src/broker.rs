//! The broker: every capture and handoff asks it first. It keeps the user's
//! grants, keyed by client × source app × destination × type and allowed once
//! or always, and the never-capture list. The engine enforces what it says;
//! an agent's instructions never decide.
//!
//! ```text
//! <data>/broker/grants.toml  what the user allowed
//! <data>/broker/never.toml   the user's additions to the never-capture list
//! ```

use std::path::PathBuf;
use std::sync::Mutex;

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

use crate::paths::Paths;
use crate::state;

/// Keeps read-modify-write updates from interleaving within this process.
static WRITE: Mutex<()> = Mutex::new(());

/// The built-in never-capture list.
const BUILT_IN: &str = include_str!("never_capture.toml");

/// The type family of screenshots and images.
pub const IMAGES: &str = "image/*";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    /// For the next capture only.
    Once,
    Always,
}

impl Scope {
    pub fn parse(text: &str) -> Option<Scope> {
        match text {
            "once" => Some(Scope::Once),
            "always" => Some(Scope::Always),
            _ => None,
        }
    }
}

/// One thing the user allowed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Grant {
    /// Who may ask, e.g. `sunday`.
    pub client: String,
    /// The source app's key (see `focus::Window::key`), or `openagora:<id>`
    /// for an app in an OpenAgora tab.
    pub source: String,
    /// The source app's name, for people.
    pub source_name: String,
    /// Who receives what is captured, e.g. `sunday`.
    pub destination: String,
    /// A type family, e.g. `image/*`.
    #[serde(rename = "type")]
    pub family: String,
    pub scope: Scope,
    pub granted_at: u64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Grants {
    #[serde(default, rename = "grant")]
    grants: Vec<Grant>,
}

/// A never-capture rule: an app, or words in window titles.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    pub name: String,
    /// App names or keys.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub apps: Vec<String>,
    /// Words in a window title.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub titles: Vec<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Rules {
    #[serde(default, rename = "rule")]
    rules: Vec<Rule>,
}

/// What OpenAgora knows about the app something would be captured from.
#[derive(Debug, Clone, Copy)]
pub struct Source<'a> {
    pub app: &'a str,
    pub key: &'a str,
    pub title: Option<&'a str>,
    pub exe: Option<&'a str>,
}

fn grants_file(paths: &Paths) -> PathBuf {
    paths.root.join("broker").join("grants.toml")
}

fn never_file(paths: &Paths) -> PathBuf {
    paths.root.join("broker").join("never.toml")
}

/// "1Password 7" and "1password" are the same app.
fn normalize(name: &str) -> String {
    let lower = name.trim().to_lowercase();
    match lower.rsplit_once(' ') {
        Some((head, tail))
            if !head.is_empty() && !tail.is_empty() && tail.chars().all(|c| c.is_ascii_digit()) =>
        {
            head.trim_end().to_string()
        }
        _ => lower,
    }
}

impl Rule {
    pub fn matches(&self, source: &Source) -> bool {
        let app = normalize(source.app);
        let key = source.key.trim().to_lowercase();
        let exe = source
            .exe
            .and_then(|e| e.rsplit(['/', '\\']).next())
            .map(str::to_lowercase);
        let by_app =
            self.apps.iter().map(|a| normalize(a)).any(|a| {
                !a.is_empty() && (a == app || a == key || exe.as_deref() == Some(a.as_str()))
            });
        let by_title = source.title.is_some_and(|title| {
            let title = title.to_lowercase();
            self.titles
                .iter()
                .map(|w| w.trim().to_lowercase())
                .any(|w| !w.is_empty() && title.contains(&w))
        });
        by_app || by_title
    }
}

pub fn built_in() -> Vec<Rule> {
    toml::from_str::<Rules>(BUILT_IN)
        .expect("the built-in never-capture list is valid TOML")
        .rules
}

pub fn user_rules(paths: &Paths) -> Result<Vec<Rule>> {
    Ok(state::read::<Rules>(&never_file(paths))?
        .unwrap_or_default()
        .rules)
}

/// The never-capture rule that covers this source, if any.
pub fn never(paths: &Paths, source: &Source) -> Result<Option<String>> {
    let rules = built_in().into_iter().chain(user_rules(paths)?);
    Ok(rules
        .into_iter()
        .find(|r| r.matches(source))
        .map(|r| r.name))
}

/// Adds the user's own rule. A rule with the same name is replaced.
pub fn add_rule(paths: &Paths, rule: Rule) -> Result<()> {
    if rule.name.trim().is_empty() {
        bail!("a never-capture rule needs a name");
    }
    if rule
        .apps
        .iter()
        .chain(&rule.titles)
        .all(|v| v.trim().is_empty())
    {
        bail!("a never-capture rule needs an app or words in a window title");
    }
    let _guard = WRITE.lock().unwrap();
    let mut rules = state::read::<Rules>(&never_file(paths))?.unwrap_or_default();
    rules.rules.retain(|r| r.name != rule.name);
    rules.rules.push(rule);
    state::write(&never_file(paths), &rules)
}

/// Removes one of the user's own rules. Built-in rules can't be removed.
pub fn remove_rule(paths: &Paths, name: &str) -> Result<bool> {
    let _guard = WRITE.lock().unwrap();
    let mut rules = state::read::<Rules>(&never_file(paths))?.unwrap_or_default();
    let before = rules.rules.len();
    rules.rules.retain(|r| r.name != name);
    if rules.rules.len() == before {
        if built_in().iter().any(|r| r.name == name) {
            bail!("{name:?} is on OpenAgora's built-in never-capture list, which can't be changed");
        }
        return Ok(false);
    }
    state::write(&never_file(paths), &rules)?;
    Ok(true)
}

pub fn grants(paths: &Paths) -> Result<Vec<Grant>> {
    Ok(state::read::<Grants>(&grants_file(paths))?
        .unwrap_or_default()
        .grants)
}

fn same_key(g: &Grant, client: &str, source: &str, destination: &str, family: &str) -> bool {
    g.client == client && g.source == source && g.destination == destination && g.family == family
}

/// Whether the user allowed this, and how. "Always" wins over "once".
pub fn allowed(
    paths: &Paths,
    client: &str,
    source: &str,
    destination: &str,
    family: &str,
) -> Result<Option<Scope>> {
    let grants = grants(paths)?;
    let mut scopes = grants
        .iter()
        .filter(|g| same_key(g, client, source, destination, family))
        .map(|g| g.scope);
    let first = scopes.next();
    Ok(match first {
        Some(Scope::Always) => Some(Scope::Always),
        Some(Scope::Once) if scopes.any(|s| s == Scope::Always) => Some(Scope::Always),
        other => other,
    })
}

/// Records a grant, replacing one with the same key and scope.
pub fn grant(paths: &Paths, grant: Grant) -> Result<()> {
    let _guard = WRITE.lock().unwrap();
    let mut grants = state::read::<Grants>(&grants_file(paths))?.unwrap_or_default();
    grants.grants.retain(|g| {
        !(same_key(
            g,
            &grant.client,
            &grant.source,
            &grant.destination,
            &grant.family,
        ) && g.scope == grant.scope)
    });
    grants.grants.push(grant);
    state::write(&grants_file(paths), &grants)
}

/// Uses up a "once" grant.
pub fn use_once(
    paths: &Paths,
    client: &str,
    source: &str,
    destination: &str,
    family: &str,
) -> Result<()> {
    let _guard = WRITE.lock().unwrap();
    let mut grants = state::read::<Grants>(&grants_file(paths))?.unwrap_or_default();
    let before = grants.grants.len();
    grants
        .grants
        .retain(|g| !(same_key(g, client, source, destination, family) && g.scope == Scope::Once));
    if grants.grants.len() != before {
        state::write(&grants_file(paths), &grants)?;
    }
    Ok(())
}

/// Takes back everything allowed for an app (by key or name). Returns how
/// many grants went.
pub fn revoke(paths: &Paths, app: &str) -> Result<usize> {
    let _guard = WRITE.lock().unwrap();
    let mut grants = state::read::<Grants>(&grants_file(paths))?.unwrap_or_default();
    let before = grants.grants.len();
    let wanted = normalize(app);
    grants
        .grants
        .retain(|g| g.source != app && normalize(&g.source_name) != wanted);
    let removed = before - grants.grants.len();
    if removed > 0 {
        state::write(&grants_file(paths), &grants)?;
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source<'a>(app: &'a str, key: &'a str, title: Option<&'a str>) -> Source<'a> {
        Source {
            app,
            key,
            title,
            exe: None,
        }
    }

    fn paths() -> (tempfile::TempDir, Paths) {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths {
            root: dir.path().to_path_buf(),
        };
        (dir, paths)
    }

    #[test]
    fn never_captures_password_managers_and_private_windows() {
        let (_dir, paths) = paths();
        let never = |s: Source| never(&paths, &s).unwrap();
        assert_eq!(
            never(source("1Password 7", "com.agilebits.onepassword7", None)).as_deref(),
            Some("1Password")
        );
        assert_eq!(
            never(source("1password", "1password.exe", None)).as_deref(),
            Some("1Password")
        );
        assert_eq!(
            never(source("Bitwarden", "com.bitwarden.desktop", Some("Vault"))).as_deref(),
            Some("Bitwarden")
        );
        assert_eq!(
            never(Source {
                app: "KeePassXC",
                key: "x",
                title: None,
                exe: Some(r"C:\Apps\KeePassXC\keepassxc.exe"),
            })
            .as_deref(),
            Some("KeePassXC")
        );
        assert_eq!(
            never(source(
                "Google Chrome",
                "chrome.exe",
                Some("New Tab - Google Chrome (Incognito)")
            ))
            .as_deref(),
            Some("Private browser windows")
        );
        assert_eq!(
            never(source(
                "Firefox",
                "firefox",
                Some("Mozilla Firefox Private Browsing")
            ))
            .as_deref(),
            Some("Private browser windows")
        );
        assert_eq!(
            never(source(
                "Microsoft Edge",
                "msedge.exe",
                Some("Bing - [InPrivate]")
            ))
            .as_deref(),
            Some("Private browser windows")
        );
        // An ordinary browser window, and a title that only mentions passwords.
        assert_eq!(
            never(source(
                "Google Chrome",
                "com.google.Chrome",
                Some("GitHub - Google Chrome")
            )),
            None
        );
        assert_eq!(
            never(source("Notes", "com.apple.Notes", Some("Password ideas"))),
            None
        );
        // An unknown title can't be checked for private windows.
        assert_eq!(never(source("Firefox", "firefox", None)), None);
    }

    #[test]
    fn users_extend_the_never_list() {
        let (_dir, paths) = paths();
        let banking = source("Firefox", "firefox", Some("My Bank — Mozilla Firefox"));
        assert_eq!(never(&paths, &banking).unwrap(), None);
        add_rule(
            &paths,
            Rule {
                name: "Bank".into(),
                apps: vec![],
                titles: vec!["my bank".into()],
            },
        )
        .unwrap();
        add_rule(
            &paths,
            Rule {
                name: "Signal".into(),
                apps: vec!["Signal".into()],
                titles: vec![],
            },
        )
        .unwrap();
        assert_eq!(never(&paths, &banking).unwrap().as_deref(), Some("Bank"));
        assert_eq!(
            never(
                &paths,
                &source("Signal", "org.whispersystems.signal-desktop", None)
            )
            .unwrap()
            .as_deref(),
            Some("Signal")
        );
        assert!(remove_rule(&paths, "Bank").unwrap());
        assert!(!remove_rule(&paths, "Bank").unwrap());
        assert!(remove_rule(&paths, "1Password").is_err());
        assert_eq!(never(&paths, &banking).unwrap(), None);
        assert!(
            add_rule(
                &paths,
                Rule {
                    name: "Empty".into(),
                    apps: vec![" ".into()],
                    titles: vec![]
                }
            )
            .is_err()
        );
    }

    #[test]
    fn grants_once_and_always() {
        let (_dir, paths) = paths();
        let grant_for = |source: &str, scope| Grant {
            client: "sunday".into(),
            source: source.into(),
            source_name: "Firefox".into(),
            destination: "sunday".into(),
            family: IMAGES.into(),
            scope,
            granted_at: 1,
        };
        let allowed = |source: &str| allowed(&paths, "sunday", source, "sunday", IMAGES).unwrap();
        assert_eq!(allowed("firefox"), None);
        grant(&paths, grant_for("firefox", Scope::Once)).unwrap();
        assert_eq!(allowed("firefox"), Some(Scope::Once));
        // Another client, destination or type isn't covered.
        assert_eq!(
            super::allowed(&paths, "codex", "firefox", "sunday", IMAGES).unwrap(),
            None
        );
        assert_eq!(
            super::allowed(&paths, "sunday", "firefox", "codex", IMAGES).unwrap(),
            None
        );
        assert_eq!(
            super::allowed(&paths, "sunday", "firefox", "sunday", "text/*").unwrap(),
            None
        );
        use_once(&paths, "sunday", "firefox", "sunday", IMAGES).unwrap();
        assert_eq!(allowed("firefox"), None);
        grant(&paths, grant_for("firefox", Scope::Always)).unwrap();
        grant(&paths, grant_for("firefox", Scope::Always)).unwrap();
        assert_eq!(grants(&paths).unwrap().len(), 1);
        use_once(&paths, "sunday", "firefox", "sunday", IMAGES).unwrap();
        assert_eq!(allowed("firefox"), Some(Scope::Always));
        assert_eq!(revoke(&paths, "Firefox").unwrap(), 1);
        assert_eq!(allowed("firefox"), None);
    }
}
