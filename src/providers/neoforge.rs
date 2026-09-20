use anyhow::{Context, Result};

use super::{Artifact, JavaHint, Layout, Version, java_for_game_version};

const MAVEN: &str = "https://maven.neoforged.net/releases/net/neoforged/neoforge";

pub async fn versions(http: &reqwest::Client) -> Result<Vec<Version>> {
    let xml = http
        .get(format!("{MAVEN}/maven-metadata.xml"))
        .send()
        .await
        .context("asking NeoForge for its versions")?
        .error_for_status()?
        .text()
        .await
        .context("reading NeoForge's version list")?;

    Ok(listed(&xml)
        .into_iter()
        .map(|id| Version {
            label: format!("{} (NeoForge {id})", minecraft_version(&id)),
            stable: !id.contains('-'),
            id,
        })
        .collect())
}

pub async fn resolve(http: &reqwest::Client, version: &str) -> Result<Artifact> {
    let minimum = java_for_game_version(http, &minecraft_version(version)).await;

    Ok(Artifact {
        url: format!("{MAVEN}/{version}/neoforge-{version}-installer.jar"),
        file_name: format!("neoforge-{version}-installer.jar"),
        sha1: None,
        sha256: None,
        layout: Layout::Installer {
            neoforge_version: version.to_string(),
        },
        version: version.to_string(),
        java: JavaHint {
            minimum,
            any_newer: false,
            flags: Vec::new(),
        },
    })
}

/// Pulls the version elements out of the Maven metadata, which is too small to be
/// worth an XML parser.
fn listed(xml: &str) -> Vec<String> {
    xml.split("<version>")
        .skip(1)
        .filter_map(|rest| rest.split_once("</version>"))
        .map(|(value, _)| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect()
}

/// NeoForge numbers itself after the Minecraft release it targets: three parts for
/// the 1.x releases, four since Minecraft moved to calendar versions.
fn minecraft_version(neoforge: &str) -> String {
    let base = neoforge.split('-').next().unwrap_or(neoforge);
    let parts: Vec<&str> = base.split('.').collect();
    match parts.as_slice() {
        [major, minor, "0", _] => format!("{major}.{minor}"),
        [major, minor, patch, _] => format!("{major}.{minor}.{patch}"),
        [minor, "0", _] => format!("1.{minor}"),
        [minor, patch, _] => format!("1.{minor}.{patch}"),
        _ => base.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_versions_out_of_maven_metadata() {
        let xml =
            "<versions><version>21.1.251</version><version>26.3.0.7-beta</version></versions>";
        assert_eq!(listed(xml), ["21.1.251", "26.3.0.7-beta"]);
    }

    #[test]
    fn names_the_minecraft_release_it_targets() {
        assert_eq!(minecraft_version("21.1.251"), "1.21.1");
        assert_eq!(minecraft_version("21.0.14"), "1.21");
        assert_eq!(minecraft_version("26.3.0.7-beta"), "26.3");
        assert_eq!(minecraft_version("26.1.2.108"), "26.1.2");
    }
}
