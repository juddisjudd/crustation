use anyhow::{Context, Result, bail};
use serde::Deserialize;

use super::{Artifact, JavaHint, Layout, Version};

const MANIFEST: &str = "https://launchermeta.mojang.com/mc/game/version_manifest_v2.json";

#[derive(Deserialize)]
struct Manifest {
    versions: Vec<Entry>,
}

#[derive(Deserialize)]
struct Entry {
    id: String,
    #[serde(rename = "type")]
    kind: String,
    url: String,
}

async fn manifest(http: &reqwest::Client) -> Result<Manifest> {
    http.get(MANIFEST)
        .send()
        .await
        .context("asking Mojang for the version manifest")?
        .error_for_status()?
        .json()
        .await
        .context("reading Mojang's version manifest")
}

pub async fn versions(http: &reqwest::Client) -> Result<Vec<Version>> {
    Ok(manifest(http)
        .await?
        .versions
        .into_iter()
        .map(|entry| Version {
            label: entry.id.clone(),
            id: entry.id,
            stable: entry.kind == "release",
        })
        .collect())
}

#[derive(Deserialize)]
struct Detail {
    downloads: Downloads,
    #[serde(rename = "javaVersion")]
    java_version: Option<JavaVersion>,
}

#[derive(Deserialize)]
struct JavaVersion {
    #[serde(rename = "majorVersion")]
    major_version: u32,
}

#[derive(Deserialize)]
struct Downloads {
    server: Option<Download>,
}

#[derive(Deserialize)]
struct Download {
    url: String,
    sha1: String,
}

async fn detail(http: &reqwest::Client, version: &str) -> Result<Detail> {
    let entry = manifest(http)
        .await?
        .versions
        .into_iter()
        .find(|entry| entry.id == version)
        .with_context(|| format!("Minecraft {version} is not in Mojang's manifest"))?;

    http.get(&entry.url)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await
        .with_context(|| format!("reading the manifest for Minecraft {version}"))
}

/// The Java a release asks for, which the forks reuse for the same game version.
pub(super) async fn java_major(http: &reqwest::Client, version: &str) -> Option<u32> {
    detail(http, version)
        .await
        .ok()?
        .java_version
        .map(|java| java.major_version)
}

pub async fn resolve(http: &reqwest::Client, version: &str) -> Result<Artifact> {
    let detail = detail(http, version).await?;
    let minimum = detail.java_version.map(|java| java.major_version);

    let Some(server) = detail.downloads.server else {
        bail!("Mojang does not publish a server for Minecraft {version}");
    };

    Ok(Artifact {
        url: server.url,
        file_name: "server.jar".into(),
        sha1: Some(server.sha1),
        sha256: None,
        layout: Layout::Jar,
        version: version.to_string(),
        java: JavaHint {
            minimum,
            any_newer: false,
            flags: Vec::new(),
        },
    })
}
