use anyhow::{Context, Result};
use serde::Deserialize;

use super::{Artifact, JavaHint, Layout, Version, java_for_game_version};

const META: &str = "https://meta.fabricmc.net/v2/versions";

#[derive(Deserialize)]
struct Game {
    version: String,
    stable: bool,
}

pub async fn versions(http: &reqwest::Client) -> Result<Vec<Version>> {
    let games: Vec<Game> = http
        .get(format!("{META}/game"))
        .send()
        .await
        .context("asking Fabric for its game versions")?
        .error_for_status()?
        .json()
        .await
        .context("reading Fabric's game version list")?;

    Ok(games
        .into_iter()
        .map(|game| Version {
            label: game.version.clone(),
            id: game.version,
            stable: game.stable,
        })
        .collect())
}

#[derive(Deserialize)]
struct Component {
    version: String,
    stable: bool,
}

/// The newest stable entry, or the newest of any kind when none is marked stable.
async fn newest(http: &reqwest::Client, what: &str) -> Result<String> {
    let entries: Vec<Component> = http
        .get(format!("{META}/{what}"))
        .send()
        .await
        .with_context(|| format!("asking Fabric for its {what} versions"))?
        .error_for_status()?
        .json()
        .await
        .with_context(|| format!("reading Fabric's {what} versions"))?;

    entries
        .iter()
        .find(|entry| entry.stable)
        .or_else(|| entries.first())
        .map(|entry| entry.version.clone())
        .with_context(|| format!("Fabric published no {what} versions"))
}

pub async fn resolve(http: &reqwest::Client, version: &str) -> Result<Artifact> {
    let loader = newest(http, "loader").await?;
    let installer = newest(http, "installer").await?;

    Ok(Artifact {
        url: format!("{META}/loader/{version}/{loader}/{installer}/server/jar"),
        file_name: "fabric-server-launch.jar".into(),
        sha1: None,
        sha256: None,
        layout: Layout::Jar,
        version: version.to_string(),
        java: JavaHint {
            minimum: java_for_game_version(http, version).await,
            any_newer: false,
            flags: Vec::new(),
        },
    })
}
