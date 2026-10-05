use anyhow::{Result, bail};

use super::{Artifact, JavaHint, Layout, Version};

/// Steam hosts Rust as branches of one app rather than numbered releases.
const BRANCHES: [(&str, &str, bool); 3] = [
    ("release", "Release", true),
    ("staging", "Staging", false),
    ("aux02", "Aux (testing)", false),
];

pub fn versions() -> Vec<Version> {
    BRANCHES
        .iter()
        .map(|(id, label, stable)| Version {
            id: id.to_string(),
            label: label.to_string(),
            stable: *stable,
        })
        .collect()
}

pub fn resolve(version: &str) -> Result<Artifact> {
    let Some((branch, _, _)) = BRANCHES.iter().find(|(id, _, _)| *id == version) else {
        bail!("Rust has no branch called '{version}'.");
    };

    Ok(Artifact {
        // SteamCMD does the fetching, so there is no link to give.
        url: String::new(),
        file_name: String::new(),
        sha1: None,
        sha256: None,
        layout: Layout::SteamCmd {
            app_id: crate::rust_game::APP_ID,
            branch: Some(branch.to_string()),
            executable: crate::rust_game::executable().to_string(),
        },
        version: branch.to_string(),
        java: JavaHint::default(),
    })
}
