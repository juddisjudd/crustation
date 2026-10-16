//! Wine, for the dedicated servers that only ship for Windows.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use tokio::process::Command;

/// One prefix for every Wine server, under the config volume beside SteamCMD.
pub fn prefix(config: &Path) -> PathBuf {
    config.join("wine")
}

/// The overrides stop Wine offering to install Mono and Gecko, which nobody is there to answer.
pub fn environment(prefix: &Path) -> Vec<(&'static str, String)> {
    vec![
        ("WINEPREFIX", prefix.to_string_lossy().into_owned()),
        ("WINEARCH", "win64".into()),
        ("WINEDEBUG", "-all".into()),
        ("WINEDLLOVERRIDES", "mscoree,mshtml=".into()),
    ]
}

/// Checks Wine is here and makes the prefix now, so a server's first start is not spent on it.
pub async fn prepare(prefix: &Path) -> Result<String> {
    let version = Command::new("wine")
        .arg("--version")
        .stdin(Stdio::null())
        .output()
        .await
        .context("this host has no Wine, which a Windows-only server needs on Linux")?;
    if !version.status.success() {
        bail!("Wine is installed but would not say its version");
    }
    let version = String::from_utf8_lossy(&version.stdout).trim().to_string();

    if tokio::fs::try_exists(prefix.join("system.reg"))
        .await
        .unwrap_or(false)
    {
        return Ok(version);
    }
    tokio::fs::create_dir_all(prefix)
        .await
        .with_context(|| format!("creating {}", prefix.display()))?;
    let mut boot = Command::new("wineboot");
    boot.arg("--init")
        .envs(environment(prefix))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let status = tokio::time::timeout(Duration::from_secs(300), boot.status())
        .await
        .context("Wine took too long to set up its prefix")?
        .context("running wineboot")?;
    if !status.success() {
        bail!("wineboot could not set up {}", prefix.display());
    }
    // wineboot returns before the files are flushed; wineserver -w waits for it to finish.
    let mut settle = Command::new("wineserver");
    settle
        .arg("-w")
        .envs(environment(prefix))
        .stdin(Stdio::null())
        .kill_on_drop(true);
    let _ = tokio::time::timeout(Duration::from_secs(120), settle.status()).await;
    Ok(version)
}
