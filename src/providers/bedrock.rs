use anyhow::{Context, Result, bail};
use serde::Deserialize;

use super::{Artifact, JavaHint, Layout, Version, bedrock_executable};

const LINKS: &str = "https://net-secondary.web.minecraft-services.net/api/v1.0/download/links";

#[derive(Deserialize)]
struct Response {
    result: Links,
}

#[derive(Deserialize)]
struct Links {
    links: Vec<Link>,
}

#[derive(Deserialize)]
struct Link {
    #[serde(rename = "downloadType")]
    download_type: String,
    #[serde(rename = "downloadUrl")]
    download_url: String,
}

/// Mojang publishes the current release and preview only, for this host's platform.
async fn current(http: &reqwest::Client) -> Result<Vec<(bool, String)>> {
    let response: Response = http
        .get(LINKS)
        .send()
        .await
        .context("asking Mojang for the Bedrock downloads")?
        .error_for_status()?
        .json()
        .await
        .context("reading Mojang's Bedrock download list")?;

    let (release, preview) = if cfg!(windows) {
        ("serverBedrockWindows", "serverBedrockPreviewWindows")
    } else {
        ("serverBedrockLinux", "serverBedrockPreviewLinux")
    };

    Ok(response
        .result
        .links
        .into_iter()
        .filter_map(|link| match link.download_type.as_str() {
            found if found == release => Some((true, link.download_url)),
            found if found == preview => Some((false, link.download_url)),
            _ => None,
        })
        .collect())
}

pub async fn versions(http: &reqwest::Client) -> Result<Vec<Version>> {
    Ok(current(http)
        .await?
        .into_iter()
        .filter_map(|(stable, url)| {
            let id = version_in(&url)?;
            Some(Version {
                label: match stable {
                    true => id.clone(),
                    false => format!("{id} (preview)"),
                },
                id,
                stable,
            })
        })
        .collect())
}

pub async fn resolve(http: &reqwest::Client, version: &str) -> Result<Artifact> {
    let Some((_, url)) = current(http)
        .await?
        .into_iter()
        .find(|(_, url)| version_in(url).as_deref() == Some(version))
    else {
        bail!(
            "Mojang publishes the current Bedrock server only. Paste the download URL for {version}, or upload the zip."
        );
    };

    Ok(Artifact {
        url,
        file_name: format!("bedrock-server-{version}.zip"),
        sha1: None,
        sha256: None,
        layout: Layout::Zip {
            executable: bedrock_executable().to_string(),
        },
        version: version.to_string(),
        // Bedrock is a native binary; no JVM is involved.
        java: JavaHint::default(),
    })
}

/// Turns a download name such as bedrock-server-1.26.51.1.zip into its version.
fn version_in(url: &str) -> Option<String> {
    let name = url.rsplit('/').next()?;
    let rest = name.strip_prefix("bedrock-server-")?;
    Some(rest.strip_suffix(".zip")?.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_version_out_of_the_download_name() {
        let url = "https://example.test/bin-linux/bedrock-server-1.26.51.1.zip";
        assert_eq!(version_in(url).as_deref(), Some("1.26.51.1"));
        assert_eq!(version_in("https://example.test/other.zip"), None);
    }
}
