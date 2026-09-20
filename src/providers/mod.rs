mod bedrock;
mod fabric;
mod neoforge;
mod paper;
mod purpur;
mod vanilla;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Result, bail};
use serde::Serialize;
use tokio::sync::Mutex;

/// One installable server flavour.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Info {
    pub id: &'static str,
    pub name: &'static str,
    pub kind: &'static str,
    pub summary: &'static str,
    pub default_port: u16,
    pub needs_java: bool,
}

pub const ALL: [Info; 6] = [
    Info {
        id: "vanilla",
        name: "Vanilla",
        kind: "minecraft_java",
        summary: "Mojang's own server, with no plugins or mods.",
        default_port: 25565,
        needs_java: true,
    },
    Info {
        id: "paper",
        name: "Paper",
        kind: "minecraft_java",
        summary: "Faster vanilla that runs Bukkit and Spigot plugins.",
        default_port: 25565,
        needs_java: true,
    },
    Info {
        id: "purpur",
        name: "Purpur",
        kind: "minecraft_java",
        summary: "Paper with more of the game exposed as settings.",
        default_port: 25565,
        needs_java: true,
    },
    Info {
        id: "fabric",
        name: "Fabric",
        kind: "minecraft_java",
        summary: "Light mod loader. The launcher fetches the rest on first start.",
        default_port: 25565,
        needs_java: true,
    },
    Info {
        id: "neoforge",
        name: "NeoForge",
        kind: "minecraft_java",
        summary: "Mod loader for Forge-style mods. Installs itself on creation.",
        default_port: 25565,
        needs_java: true,
    },
    Info {
        id: "bedrock",
        name: "Bedrock",
        kind: "minecraft_bedrock",
        summary: "Mojang's dedicated server for phones, consoles and Windows.",
        default_port: 19132,
        needs_java: false,
    },
];

pub fn find(id: &str) -> Option<Info> {
    ALL.into_iter().find(|info| info.id == id)
}

#[derive(Debug, Clone, Serialize)]
pub struct Version {
    pub id: String,
    pub label: String,
    pub stable: bool,
}

/// What to download and what the download turns out to be.
pub struct Artifact {
    pub url: String,
    pub file_name: String,
    pub sha1: Option<String>,
    pub sha256: Option<String>,
    pub layout: Layout,
    /// The version actually resolved, which may be more precise than the request.
    pub version: String,
    pub java: JavaHint,
}

/// What a version needs from the JVM, when the project says so. Everything here is
/// best effort: knowing nothing means the panel leaves the choice alone.
#[derive(Debug, Default)]
pub struct JavaHint {
    /// The Java major version the project names for this release.
    pub minimum: Option<u32>,
    /// Set when the project states a floor it supports anything above. Mojang
    /// instead names the one runtime a release was built against, and old releases
    /// break on anything much newer, so that number is a target rather than a floor.
    pub any_newer: bool,
    /// Flags the project recommends for itself.
    pub flags: Vec<String>,
}

/// Mojang records the Java each release needs, and anything built on a release
/// needs the same, so the forks borrow the answer rather than guess at it.
pub async fn java_for_game_version(http: &reqwest::Client, version: &str) -> Option<u32> {
    vanilla::java_major(http, version).await
}

pub enum Layout {
    /// The download is the server jar, ready to run.
    Jar,
    /// A zip holding the whole server folder.
    Zip { executable: String },
    /// An installer that has to be run once before the server exists.
    Installer { neoforge_version: String },
}

/// Version lists, kept for a few minutes so opening the wizard does not hammer upstream.
#[derive(Clone, Default)]
pub struct Catalogue {
    entries: Arc<Mutex<HashMap<String, Cached>>>,
}

struct Cached {
    at: Instant,
    versions: Vec<Version>,
}

const TTL: Duration = Duration::from_secs(600);

impl Catalogue {
    pub async fn versions(&self, http: &reqwest::Client, provider: &str) -> Result<Vec<Version>> {
        if let Some(cached) = self.entries.lock().await.get(provider)
            && cached.at.elapsed() < TTL
        {
            return Ok(cached.versions.clone());
        }

        let mut versions = match provider {
            "vanilla" => vanilla::versions(http).await?,
            "paper" => paper::versions(http).await?,
            "purpur" => purpur::versions(http).await?,
            "fabric" => fabric::versions(http).await?,
            "neoforge" => neoforge::versions(http).await?,
            "bedrock" => bedrock::versions(http).await?,
            other => bail!("no provider called '{other}'"),
        };
        sort_newest_first(&mut versions);

        self.entries.lock().await.insert(
            provider.to_string(),
            Cached {
                at: Instant::now(),
                versions: versions.clone(),
            },
        );
        Ok(versions)
    }
}

pub async fn resolve(http: &reqwest::Client, provider: &str, version: &str) -> Result<Artifact> {
    match provider {
        "vanilla" => vanilla::resolve(http, version).await,
        "paper" => paper::resolve(http, version).await,
        "purpur" => purpur::resolve(http, version).await,
        "fabric" => fabric::resolve(http, version).await,
        "neoforge" => neoforge::resolve(http, version).await,
        "bedrock" => bedrock::resolve(http, version).await,
        other => bail!("no provider called '{other}'"),
    }
}

/// The file a Bedrock zip is started from, which differs by host.
pub fn bedrock_executable() -> &'static str {
    if cfg!(windows) {
        "bedrock_server.exe"
    } else {
        "bedrock_server"
    }
}

/// Newest first, with releases ahead of the pre-releases they share a number with.
fn sort_newest_first(versions: &mut [Version]) {
    versions.sort_by_key(|entry| std::cmp::Reverse(key(&entry.id)));
}

fn key(version: &str) -> (Vec<u32>, bool, String) {
    let (base, suffix) = match version.split_once('-') {
        Some((base, suffix)) => (base, suffix.to_string()),
        None => (version, String::new()),
    };
    let numbers = base
        .split('.')
        .map(|part| part.parse().unwrap_or(0))
        .collect();
    (numbers, suffix.is_empty(), suffix)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(id: &str) -> Version {
        Version {
            id: id.to_string(),
            label: id.to_string(),
            stable: true,
        }
    }

    #[test]
    fn orders_calendar_and_classic_versions_newest_first() {
        let mut versions = [
            version("1.21.4"),
            version("26.1.2"),
            version("26.3"),
            version("1.21.11"),
            version("26.2"),
        ];
        sort_newest_first(&mut versions);
        let ids: Vec<_> = versions.iter().map(|entry| entry.id.as_str()).collect();
        assert_eq!(ids, ["26.3", "26.2", "26.1.2", "1.21.11", "1.21.4"]);
    }

    #[test]
    fn a_release_outranks_its_pre_releases() {
        let mut versions = [version("26.3-rc-3"), version("26.3"), version("26.3-pre5")];
        sort_newest_first(&mut versions);
        assert_eq!(versions[0].id, "26.3");
    }
}
