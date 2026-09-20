use anyhow::{Context, Result, bail};
use serde::Deserialize;

use super::{Artifact, JavaHint, Layout, Version};

const PROJECT: &str = "https://fill.papermc.io/v3/projects/paper";

#[derive(Deserialize)]
struct Versions {
    versions: Vec<Entry>,
}

#[derive(Deserialize)]
struct Entry {
    version: Detail,
}

#[derive(Deserialize)]
struct Detail {
    id: String,
    support: Support,
    java: Option<Java>,
}

#[derive(Deserialize)]
struct Support {
    status: String,
}

/// Paper is the only project that publishes both the Java it needs and the flags
/// it wants to be run with, so its answer is the one the whole install trusts.
#[derive(Deserialize)]
struct Java {
    version: JavaVersion,
    flags: Flags,
}

#[derive(Deserialize)]
struct JavaVersion {
    minimum: u32,
}

#[derive(Deserialize)]
struct Flags {
    #[serde(default)]
    recommended: Vec<String>,
}

#[derive(Deserialize)]
struct OneVersion {
    version: Detail,
}

pub async fn versions(http: &reqwest::Client) -> Result<Vec<Version>> {
    let listed: Versions = http
        .get(format!("{PROJECT}/versions"))
        .send()
        .await
        .context("asking PaperMC for its versions")?
        .error_for_status()?
        .json()
        .await
        .context("reading PaperMC's version list")?;

    Ok(listed
        .versions
        .into_iter()
        .map(|entry| Version {
            label: entry.version.id.clone(),
            id: entry.version.id,
            stable: entry.version.support.status == "SUPPORTED",
        })
        .collect())
}

#[derive(Deserialize)]
struct Build {
    id: i64,
    channel: String,
    downloads: std::collections::HashMap<String, Download>,
}

#[derive(Deserialize)]
struct Download {
    name: String,
    url: String,
    checksums: Checksums,
}

#[derive(Deserialize)]
struct Checksums {
    sha256: Option<String>,
}

/// Best effort: a missing answer only means the install picks for itself.
async fn java(http: &reqwest::Client, version: &str) -> JavaHint {
    let found: Option<OneVersion> = async {
        http.get(format!("{PROJECT}/versions/{version}"))
            .send()
            .await
            .ok()?
            .error_for_status()
            .ok()?
            .json()
            .await
            .ok()
    }
    .await;

    match found.and_then(|one| one.version.java) {
        Some(java) => JavaHint {
            minimum: Some(java.version.minimum),
            // Paper calls this a minimum and supports running above it.
            any_newer: true,
            flags: java.flags.recommended,
        },
        None => JavaHint::default(),
    }
}

pub async fn resolve(http: &reqwest::Client, version: &str) -> Result<Artifact> {
    let builds: Vec<Build> = http
        .get(format!("{PROJECT}/versions/{version}/builds"))
        .send()
        .await
        .with_context(|| format!("asking PaperMC for builds of {version}"))?
        .error_for_status()
        .with_context(|| format!("PaperMC has no builds for {version}"))?
        .json()
        .await
        .context("reading PaperMC's build list")?;

    // Newest first, but prefer a finished build over today's alpha.
    let build = builds
        .iter()
        .find(|build| build.channel == "STABLE" || build.channel == "RECOMMENDED")
        .or_else(|| builds.first())
        .with_context(|| format!("PaperMC has no builds for {version}"))?;

    let Some(download) = build.downloads.get("server:default") else {
        bail!("PaperMC build {} has no server download", build.id);
    };

    Ok(Artifact {
        url: download.url.clone(),
        file_name: download.name.clone(),
        sha1: None,
        sha256: download.checksums.sha256.clone(),
        layout: Layout::Jar,
        version: version.to_string(),
        java: java(http, version).await,
    })
}
