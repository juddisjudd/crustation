use anyhow::{Context, Result};
use serde::Deserialize;

use super::{Artifact, JavaHint, Layout, Version, java_for_game_version};

const PROJECT: &str = "https://api.purpurmc.org/v2/purpur";

#[derive(Deserialize)]
struct Project {
    versions: Vec<String>,
}

pub async fn versions(http: &reqwest::Client) -> Result<Vec<Version>> {
    let project: Project = http
        .get(PROJECT)
        .send()
        .await
        .context("asking Purpur for its versions")?
        .error_for_status()?
        .json()
        .await
        .context("reading Purpur's version list")?;

    Ok(project
        .versions
        .into_iter()
        .map(|id| Version {
            label: id.clone(),
            stable: !id.contains('-'),
            id,
        })
        .collect())
}

#[derive(Deserialize)]
struct Builds {
    builds: Latest,
}

#[derive(Deserialize)]
struct Latest {
    latest: String,
}

pub async fn resolve(http: &reqwest::Client, version: &str) -> Result<Artifact> {
    let builds: Builds = http
        .get(format!("{PROJECT}/{version}"))
        .send()
        .await
        .with_context(|| format!("asking Purpur for builds of {version}"))?
        .error_for_status()
        .with_context(|| format!("Purpur has no builds for {version}"))?
        .json()
        .await
        .context("reading Purpur's build list")?;

    let build = builds.builds.latest;
    Ok(Artifact {
        url: format!("{PROJECT}/{version}/{build}/download"),
        file_name: format!("purpur-{version}-{build}.jar"),
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
