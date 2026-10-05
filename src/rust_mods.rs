//! Oxide and Carbon, the two ways a Rust server runs plugins, and the plugins themselves.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

const OXIDE_URL: &str =
    "https://github.com/OxideMod/Oxide.Rust/releases/latest/download/Oxide.Rust-linux.zip";
/// Oxide replaces the game's own assemblies and adds this one beside them.
const OXIDE_MARK: &str = "RustDedicated_Data/Managed/Oxide.Rust.dll";
/// Carbon leaves the game alone and is loaded by Unity Doorstop from here.
const CARBON_MARK: &str = "carbon/managed/Carbon.Preloader.dll";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Framework {
    Oxide,
    Carbon,
}

impl Framework {
    fn folder(self) -> &'static str {
        match self {
            Framework::Oxide => "oxide",
            Framework::Carbon => "carbon",
        }
    }

    /// What it answers to over RCON when asked which build it is.
    pub fn version_command(self) -> &'static str {
        match self {
            Framework::Oxide => "oxide.version",
            Framework::Carbon => "c.version",
        }
    }

    /// `load`, `unload` or `reload` for one plugin, in this framework's own words.
    pub fn plugin_command(self, verb: &str, plugin: &str) -> String {
        match self {
            Framework::Oxide => format!("oxide.{verb} {plugin}"),
            Framework::Carbon => format!("c.{verb} {plugin}"),
        }
    }
}

/// Which framework a server runs: Carbon only counts when the start command loads it.
pub fn detect(directory: &Path, command: &str) -> Option<Framework> {
    let carbon_loaded = command.contains("DOORSTOP_ENABLED=1");
    if carbon_loaded && directory.join(CARBON_MARK).exists() {
        Some(Framework::Carbon)
    } else if directory.join(OXIDE_MARK).exists() {
        Some(Framework::Oxide)
    } else {
        None
    }
}

/// What Carbon's `environment.sh` exports, less the library path the command already sets.
pub fn carbon_env(directory: &Path) -> Vec<(&'static str, Option<String>)> {
    let base = std::path::absolute(directory).unwrap_or_else(|_| directory.to_path_buf());
    vec![
        ("TERM", Some("xterm".into())),
        ("DOORSTOP_ENABLED", Some("1".into())),
        (
            "DOORSTOP_TARGET_ASSEMBLY",
            Some(base.join(CARBON_MARK).to_string_lossy().into_owned()),
        ),
        (
            "LD_PRELOAD",
            Some(base.join("libdoorstop.so").to_string_lossy().into_owned()),
        ),
    ]
}

pub fn without_carbon_env() -> Vec<(&'static str, Option<String>)> {
    [
        "TERM",
        "DOORSTOP_ENABLED",
        "DOORSTOP_TARGET_ASSEMBLY",
        "LD_PRELOAD",
    ]
    .into_iter()
    .map(|key| (key, None))
    .collect()
}

/// Carbon builds one release per Rust branch; the test branches only get a debug build.
fn carbon_url(branch: &str) -> String {
    let (tag, flavour) = match branch {
        "" | "release" => ("production_build".to_string(), "Release"),
        other => (format!("rustbeta_{other}_build"), "Debug"),
    };
    format!(
        "https://github.com/CarbonCommunity/Carbon/releases/download/{tag}/Carbon.Linux.{flavour}.tar.gz"
    )
}

async fn fetch(http: &reqwest::Client, url: &str, to: &Path) -> Result<()> {
    let bytes = http
        .get(url)
        .send()
        .await
        .with_context(|| format!("fetching {url}"))?
        .error_for_status()
        .with_context(|| format!("fetching {url}"))?
        .bytes()
        .await
        .context("the download was cut short")?;
    tokio::fs::write(to, &bytes)
        .await
        .with_context(|| format!("writing {}", to.display()))
}

/// Unpacks the newest Oxide over the game's assemblies. The server has to be stopped.
pub async fn install_oxide(http: &reqwest::Client, directory: &Path) -> Result<()> {
    let archive = directory.join(".oxide-download.zip");
    fetch(http, OXIDE_URL, &archive).await?;
    let (from, into) = (archive.clone(), directory.to_path_buf());
    let unpacked = tokio::task::spawn_blocking(move || crate::files::extract_zip(&from, &into, ""))
        .await
        .context("unpacking Oxide")?;
    tokio::fs::remove_file(&archive).await.ok();
    unpacked?;
    if !directory.join(OXIDE_MARK).exists() {
        bail!("the Oxide download did not hold {OXIDE_MARK}");
    }
    Ok(())
}

/// Unpacks the Carbon build for the branch; `carbon/`, with its plugins and settings, stays.
pub async fn install_carbon(http: &reqwest::Client, directory: &Path, branch: &str) -> Result<()> {
    let archive = directory.join(".carbon-download.tar.gz");
    fetch(http, &carbon_url(branch), &archive).await?;
    let output = tokio::process::Command::new("tar")
        .arg("-xzf")
        .arg(&archive)
        .arg("-C")
        .arg(directory)
        .output()
        .await
        .context("running tar to unpack Carbon")?;
    tokio::fs::remove_file(&archive).await.ok();
    if !output.status.success() {
        bail!(
            "tar could not unpack Carbon: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    if !directory.join(CARBON_MARK).exists() {
        bail!("the Carbon download did not hold {CARBON_MARK}");
    }
    Ok(())
}

/// Deletes Oxide's own files, which Steam's check leaves behind as files it never shipped.
pub async fn remove_oxide(directory: &Path) -> Result<()> {
    let managed = directory.join("RustDedicated_Data").join("Managed");
    let mut entries = tokio::fs::read_dir(&managed)
        .await
        .context("reading the game's assemblies")?;
    while let Some(entry) = entries.next_entry().await? {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with("Oxide.") {
            tokio::fs::remove_file(entry.path())
                .await
                .with_context(|| format!("deleting {name}"))?;
        }
    }
    Ok(())
}

/// Where the framework compiles plugins from, and where a switched-off one waits.
pub fn plugin_dirs(directory: &Path, framework: Framework) -> (PathBuf, PathBuf) {
    let root = directory.join(framework.folder());
    (root.join("plugins"), root.join("plugins-disabled"))
}

/// A plugin file name: letters, digits and underscores, then `.cs`.
pub fn is_plugin_file(name: &str) -> bool {
    name.strip_suffix(".cs").is_some_and(|stem| {
        !stem.is_empty() && stem.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    })
}

#[derive(Debug, Serialize, PartialEq)]
pub struct Plugin {
    pub file: String,
    pub name: String,
    pub title: Option<String>,
    pub author: Option<String>,
    pub version: Option<String>,
    pub enabled: bool,
}

/// `[Info("Title", "Author", "1.2.3")]`, the attribute every Oxide and Carbon plugin carries.
pub fn read_info(source: &str) -> Option<(String, String, String)> {
    let start = source.find("[Info(")? + "[Info(".len();
    let end = start + source[start..].find(")]")?;
    let mut fields = source[start..end]
        .split('"')
        .skip(1)
        .step_by(2)
        .map(str::to_string);
    Some((fields.next()?, fields.next()?, fields.next()?))
}

pub async fn list(directory: &Path, framework: Framework) -> Vec<Plugin> {
    let (enabled, disabled) = plugin_dirs(directory, framework);
    let mut found = Vec::new();
    for (folder, on) in [(enabled, true), (disabled, false)] {
        let Ok(mut entries) = tokio::fs::read_dir(&folder).await else {
            continue;
        };
        while let Ok(Some(entry)) = entries.next_entry().await {
            let file = entry.file_name().to_string_lossy().into_owned();
            if !is_plugin_file(&file) {
                continue;
            }
            let source = tokio::fs::read_to_string(entry.path())
                .await
                .unwrap_or_default();
            let info = read_info(&source);
            found.push(Plugin {
                name: file.trim_end_matches(".cs").to_string(),
                title: info.as_ref().map(|(title, _, _)| title.clone()),
                author: info.as_ref().map(|(_, author, _)| author.clone()),
                version: info.map(|(_, _, version)| version),
                enabled: on,
                file,
            });
        }
    }
    found.sort_by_key(|plugin| plugin.name.to_lowercase());
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_info_attribute() {
        let source = "namespace Oxide.Plugins\n{\n    [Info(\"Whitelist\", \"Wulf/lukespragg\", \"3.3.0\")]\n    [Description(\"x\")]\n    class Whitelist : CovalencePlugin {}\n}";
        assert_eq!(
            read_info(source),
            Some(("Whitelist".into(), "Wulf/lukespragg".into(), "3.3.0".into()))
        );
        assert_eq!(read_info("class Nothing {}"), None);
    }

    #[test]
    fn checks_plugin_file_names() {
        assert!(is_plugin_file("Whitelist.cs"));
        assert!(is_plugin_file("Better_Chat2.cs"));
        assert!(!is_plugin_file("../evil.cs"));
        assert!(!is_plugin_file("Whitelist.dll"));
        assert!(!is_plugin_file(".cs"));
    }

    #[test]
    fn picks_the_carbon_build_for_the_branch() {
        assert!(carbon_url("release").ends_with("production_build/Carbon.Linux.Release.tar.gz"));
        assert!(
            carbon_url("staging").ends_with("rustbeta_staging_build/Carbon.Linux.Debug.tar.gz")
        );
        assert!(carbon_url("aux02").ends_with("rustbeta_aux02_build/Carbon.Linux.Debug.tar.gz"));
    }

    #[test]
    fn carbon_only_counts_when_the_command_loads_it() {
        let folder = std::env::temp_dir().join("crustation-rust-mods");
        std::fs::create_dir_all(folder.join("carbon/managed")).unwrap();
        std::fs::write(folder.join(CARBON_MARK), b"x").unwrap();
        assert_eq!(detect(&folder, "env ./RustDedicated"), None);
        assert_eq!(
            detect(&folder, "env DOORSTOP_ENABLED=1 ./RustDedicated"),
            Some(Framework::Carbon)
        );
        std::fs::remove_dir_all(&folder).ok();
    }
}
