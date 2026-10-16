use anyhow::{Context, Result, bail};

use super::{Artifact, JavaHint, Layout, Version};

/// Steam lists these servers on its public branch alone.
pub fn versions() -> Vec<Version> {
    vec![Version {
        id: "release".into(),
        label: "Public".into(),
        stable: true,
    }]
}

pub fn resolve(kind: &str, version: &str) -> Result<Artifact> {
    let title = crate::steam_games::find(kind).context("not a Steam game")?;
    if version != "release" {
        bail!("{} has no branch called '{version}'.", title.name);
    }
    Ok(Artifact {
        // SteamCMD does the fetching, so there is no link to give.
        url: String::new(),
        file_name: String::new(),
        sha1: None,
        sha256: None,
        layout: Layout::SteamCmd {
            app_id: title.app_id,
            branch: Some(version.to_string()),
            executable: crate::steam_games::executable(kind)
                .unwrap_or_default()
                .to_string(),
        },
        version: version.to_string(),
        java: JavaHint::default(),
    })
}
