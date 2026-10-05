//! SteamCMD: fetched from Valve once, then used to install dedicated servers.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Stdio};
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::{Child, Command};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio::sync::{Mutex, OwnedMutexGuard};

#[cfg(windows)]
const DOWNLOAD: &str = "https://steamcdn-a.akamaihd.net/client/installer/steamcmd.zip";
#[cfg(not(windows))]
const DOWNLOAD: &str = "https://steamcdn-a.akamaihd.net/client/installer/steamcmd_linux.tar.gz";

/// SteamCMD and the state it keeps live under the config volume, shared by every
/// server, so the second install does not fetch the client again.
pub fn directory(config: &Path) -> PathBuf {
    config.join("steamcmd")
}

/// Where Steam keeps its login and library state. Servers started with this as
/// their home find the client library SteamCMD linked there.
pub fn home(config: &Path) -> PathBuf {
    directory(config).join("home")
}

fn program(directory: &Path) -> PathBuf {
    directory.join(if cfg!(windows) {
        "steamcmd.exe"
    } else {
        "steamcmd.sh"
    })
}

/// Returns the SteamCMD launcher, downloading Valve's installer if it is not here yet.
pub async fn ensure(http: &reqwest::Client, directory: &Path) -> Result<PathBuf> {
    let program = program(directory);
    if tokio::fs::try_exists(&program).await.unwrap_or(false) {
        return Ok(program);
    }

    tokio::fs::create_dir_all(directory)
        .await
        .with_context(|| format!("creating {}", directory.display()))?;
    let bytes = http
        .get(DOWNLOAD)
        .send()
        .await
        .context("fetching SteamCMD")?
        .error_for_status()
        .context("fetching SteamCMD")?
        .bytes()
        .await
        .context("the SteamCMD download was cut short")?;

    let archive = directory.join(DOWNLOAD.rsplit('/').next().unwrap_or("steamcmd.archive"));
    tokio::fs::write(&archive, &bytes)
        .await
        .with_context(|| format!("writing {}", archive.display()))?;
    let unpacked = unpack(&archive, directory).await;
    tokio::fs::remove_file(&archive).await.ok();
    unpacked?;

    if !tokio::fs::try_exists(&program).await.unwrap_or(false) {
        bail!(
            "the SteamCMD download did not contain {}",
            program.display()
        );
    }
    Ok(program)
}

#[cfg(windows)]
async fn unpack(archive: &Path, into: &Path) -> Result<()> {
    let archive = archive.to_path_buf();
    let into = into.to_path_buf();
    tokio::task::spawn_blocking(move || crate::files::extract_zip(&archive, &into, ""))
        .await
        .context("unpacking SteamCMD")?
}

#[cfg(not(windows))]
async fn unpack(archive: &Path, into: &Path) -> Result<()> {
    let output = Command::new("tar")
        .arg("-xzf")
        .arg(archive)
        .arg("-C")
        .arg(into)
        .output()
        .await
        .context("running tar to unpack SteamCMD")?;
    if !output.status.success() {
        bail!(
            "tar could not unpack SteamCMD: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

/// A SteamCMD run in progress. Lines arrive as the process prints them, split on
/// carriage returns as well, since that is how it redraws its progress.
pub struct Running {
    child: Child,
    pub lines: UnboundedReceiver<String>,
    _turn: OwnedMutexGuard<()>,
}

impl Running {
    /// Call once `lines` has run dry, which is when the process has closed its output.
    pub async fn finish(mut self) -> Result<ExitStatus> {
        self.child.wait().await.context("waiting for SteamCMD")
    }
}

/// Two SteamCMD runs sharing one Steam home trip over each other's lock files.
static ONE_AT_A_TIME: LazyLock<Arc<Mutex<()>>> = LazyLock::new(Arc::default);

/// Installs or updates an app into `target`. `home` is where Steam keeps its own state.
pub async fn app_update(
    program: &Path,
    home: &Path,
    target: &Path,
    app_id: u32,
    branch: Option<&str>,
) -> Result<Running> {
    let turn = ONE_AT_A_TIME.clone().lock_owned().await;
    let mut command = Command::new(program);
    command
        .env("HOME", home)
        // The install directory has to be named before the login.
        .arg("+force_install_dir")
        .arg(target)
        .args(["+login", "anonymous", "+app_update"])
        .arg(app_id.to_string());
    if let Some(branch) = branch.filter(|branch| *branch != "release") {
        command.args(["-beta", branch]);
    }
    command
        .args(["validate", "+quit"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let mut child = command
        .spawn()
        .with_context(|| format!("starting {}", program.display()))?;
    let (sender, lines) = unbounded_channel();
    if let Some(stdout) = child.stdout.take() {
        tokio::spawn(split_lines(stdout, sender.clone()));
    }
    if let Some(stderr) = child.stderr.take() {
        tokio::spawn(split_lines(stderr, sender));
    }
    Ok(Running {
        child,
        lines,
        _turn: turn,
    })
}

async fn split_lines<R: AsyncRead + Unpin>(mut reader: R, sender: UnboundedSender<String>) {
    let mut buffer = [0u8; 4096];
    let mut line = Vec::new();
    loop {
        let read = match reader.read(&mut buffer).await {
            Ok(0) | Err(_) => break,
            Ok(read) => read,
        };
        for &byte in &buffer[..read] {
            if byte != b'\n' && byte != b'\r' {
                line.push(byte);
            } else if !line.is_empty() {
                let text = plain(&line);
                line.clear();
                if !text.is_empty() && sender.send(text).is_err() {
                    return;
                }
            }
        }
    }
    let text = plain(&line);
    if !text.is_empty() {
        sender.send(text).ok();
    }
}

fn plain(bytes: &[u8]) -> String {
    crate::supervisor::strip_escapes(&String::from_utf8_lossy(bytes))
        .trim()
        .to_string()
}

/// What SteamCMD is doing, read off one line of its output.
#[derive(Debug, PartialEq)]
pub struct Step {
    pub phase: &'static str,
    pub percent: Option<f64>,
    /// Bytes done and bytes in all, where the line says.
    pub bytes: Option<(u64, u64)>,
    pub detail: Option<String>,
}

pub fn step_in(line: &str) -> Option<Step> {
    if let Some((_, state)) = line.split_once("Update state (") {
        return update_state(state);
    }
    // SteamCMD updating itself: `[ 45%] Downloading update (1,234 of 45,000 KB)...`.
    if let Some(rest) = line.strip_prefix('[') {
        let (mark, words) = rest.split_once(']')?;
        if mark != "----" && !mark.ends_with('%') {
            return None;
        }
        return Some(Step {
            phase: "steamcmd",
            percent: mark.trim().trim_end_matches('%').parse().ok(),
            bytes: None,
            detail: Some(words.trim().trim_end_matches('.').to_string()).filter(|d| !d.is_empty()),
        });
    }
    let signing_in = [
        "Connecting anonymously",
        "Logging in user",
        "Waiting for client config",
    ]
    .iter()
    .any(|start| line.starts_with(start));
    signing_in.then_some(Step {
        phase: "signing_in",
        percent: None,
        bytes: None,
        detail: None,
    })
}

/// `0x61) downloading, progress: 45.20 (3100 / 6850)`, after `Update state (`.
fn update_state(state: &str) -> Option<Step> {
    let (_, rest) = state.split_once(") ")?;
    let (name, rest) = rest.split_once(',')?;
    let phase = match name.trim() {
        "preallocating" => "preallocating",
        "downloading" => "downloading",
        "verifying update" => "verifying",
        "committing" => "committing",
        // Reconfiguring, and checking what is already there before the download.
        _ => "resolving",
    };
    let (_, numbers) = rest.split_once("progress:")?;
    let percent = numbers.split_whitespace().next()?.parse().ok();
    let bytes = numbers
        .split_once('(')
        .and_then(|(_, pair)| pair.trim_end_matches(')').split_once('/'))
        .and_then(|(done, total)| Some((done.trim().parse().ok()?, total.trim().parse().ok()?)))
        .filter(|&(_, total): &(u64, u64)| total > 0);
    Some(Step {
        phase,
        percent,
        bytes,
        detail: None,
    })
}

/// Servers look for the Steam client library under `$HOME/.steam/sdk64`, and
/// SteamCMD keeps its own copy in `linux64`.
#[cfg(unix)]
pub async fn link_client_library(directory: &Path, home: &Path) {
    for (bits, source) in [("sdk64", "linux64"), ("sdk32", "linux32")] {
        let library = directory.join(source).join("steamclient.so");
        if !tokio::fs::try_exists(&library).await.unwrap_or(false) {
            continue;
        }
        let folder = home.join(".steam").join(bits);
        if tokio::fs::create_dir_all(&folder).await.is_err() {
            continue;
        }
        let link = folder.join("steamclient.so");
        tokio::fs::remove_file(&link).await.ok();
        tokio::fs::symlink(&library, &link).await.ok();
    }
}

#[cfg(not(unix))]
pub async fn link_client_library(_directory: &Path, _home: &Path) {}

/// The build an install holds, from the manifest SteamCMD writes beside it.
pub fn installed_build(target: &Path, app_id: u32) -> Option<u64> {
    let manifest = target
        .join("steamapps")
        .join(format!("appmanifest_{app_id}.acf"));
    let text = std::fs::read_to_string(manifest).ok()?;
    text.lines().find_map(|line| {
        let mut words = line.split('"').filter(|word| !word.trim().is_empty());
        (words.next()? == "buildid").then(|| words.next()?.parse().ok())?
    })
}

/// Steam calls the default branch `public`; the panel calls it `release`.
fn steam_branch(branch: &str) -> &str {
    match branch {
        "" | "release" => "public",
        other => other,
    }
}

const LATEST_FOR: Duration = Duration::from_secs(30 * 60);
type Builds = HashMap<(u32, String), (Instant, u64)>;
static LATEST: LazyLock<std::sync::Mutex<Builds>> = LazyLock::new(Default::default);

/// The newest build Steam has for a branch, as last asked, however long ago.
pub fn known_latest(app_id: u32, branch: &str) -> Option<u64> {
    let latest = LATEST.lock().expect("lock");
    latest
        .get(&(app_id, steam_branch(branch).to_string()))
        .map(|(_, build)| *build)
}

/// The newest build Steam has for a branch, asking SteamCMD at most every half hour.
pub async fn latest_build(program: &Path, home: &Path, app_id: u32, branch: &str) -> Result<u64> {
    let key = (app_id, steam_branch(branch).to_string());
    if let Some((at, build)) = LATEST.lock().expect("lock").get(&key)
        && at.elapsed() < LATEST_FOR
    {
        return Ok(*build);
    }

    let _turn = ONE_AT_A_TIME.lock().await;
    // SteamCMD answers from its cache unless the cache is gone.
    if let Some(directory) = program.parent() {
        std::fs::remove_file(directory.join("appcache").join("appinfo.vdf")).ok();
    }
    std::fs::remove_file(home.join("Steam").join("appcache").join("appinfo.vdf")).ok();
    let output = tokio::time::timeout(
        Duration::from_secs(120),
        Command::new(program)
            .env("HOME", home)
            .args([
                "+login",
                "anonymous",
                "+app_info_update",
                "1",
                "+app_info_print",
            ])
            .arg(app_id.to_string())
            .arg("+quit")
            .stdin(Stdio::null())
            .kill_on_drop(true)
            .output(),
    )
    .await
    .context("SteamCMD took too long to say which build is current")?
    .context("running SteamCMD")?;
    let build = parse_app_info(&String::from_utf8_lossy(&output.stdout), app_id, &key.1)
        .context("Steam did not say which build is current")?;
    LATEST
        .lock()
        .expect("lock")
        .insert(key, (Instant::now(), build));
    Ok(build)
}

/// `branches` → `<branch>` → `buildid` out of the KeyValues text `app_info_print` prints.
fn parse_app_info(text: &str, app_id: u32, branch: &str) -> Option<u64> {
    let start = text.find(&format!("\"{app_id}\""))?;
    let mut path: Vec<String> = Vec::new();
    let mut pending: Option<String> = None;
    let mut chars = text[start..].chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                let word: String = chars.by_ref().take_while(|c| *c != '"').collect();
                match pending.take() {
                    Some(key) => {
                        let at: Vec<&str> = path.iter().map(String::as_str).collect();
                        if key == "buildid" && at.ends_with(&["branches", branch]) {
                            return word.parse().ok();
                        }
                    }
                    None => pending = Some(word),
                }
            }
            '{' => path.extend(pending.take()),
            '}' => {
                path.pop();
                if path.is_empty() {
                    return None;
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_download_out_of_a_progress_line() {
        let line = "Update state (0x61) downloading, progress: 45.20 (3100000000 / 6850000000)";
        assert_eq!(
            step_in(line),
            Some(Step {
                phase: "downloading",
                percent: Some(45.2),
                bytes: Some((3_100_000_000, 6_850_000_000)),
                detail: None,
            })
        );
    }

    #[test]
    fn names_each_update_state() {
        let phase = |line: &str| step_in(line).map(|step| step.phase);
        assert_eq!(
            phase("Update state (0x11) preallocating, progress: 12.00 (1 / 8)"),
            Some("preallocating")
        );
        assert_eq!(
            phase("Update state (0x5) verifying install, progress: 3.00 (1 / 8)"),
            Some("resolving")
        );
        assert_eq!(
            phase("Update state (0x81) verifying update, progress: 60.00 (5 / 8)"),
            Some("verifying")
        );
        assert_eq!(
            phase("Update state (0x101) committing, progress: 99.00 (7 / 8)"),
            Some("committing")
        );
    }

    #[test]
    fn reads_steamcmd_updating_itself() {
        assert_eq!(
            step_in("[ 45%] Downloading update (1,234 of 45,000 KB)..."),
            Some(Step {
                phase: "steamcmd",
                percent: Some(45.0),
                bytes: None,
                detail: Some("Downloading update (1,234 of 45,000 KB)".into()),
            })
        );
        assert_eq!(
            step_in("[----] Verifying installation...").map(|step| step.percent),
            Some(None)
        );
    }

    #[test]
    fn notices_the_sign_in() {
        let step = step_in("Logging in user 'anonymous' to Steam Public...OK");
        assert_eq!(step.map(|step| step.phase), Some("signing_in"));
    }

    #[test]
    fn other_lines_are_not_steps() {
        assert_eq!(step_in("Success! App '258550' fully installed."), None);
        assert_eq!(step_in("[S_API FAIL] SteamAPI_Init() failed"), None);
    }

    const APP_INFO: &str = r#"Loading Steam API...OK
AppID : 258550, change number : 1/0, last change : Thu Jan  1 00:00:00 1970
"258550"
{
	"common"
	{
		"name"		"Rust Dedicated Server"
	}
	"depots"
	{
		"branches"
		{
			"public"
			{
				"buildid"		"25681086"
				"timeupdated"		"1791000000"
			}
			"staging"
			{
				"buildid"		"25681182"
				"description"		"Staging"
			}
		}
	}
}
"#;

    #[test]
    fn reads_the_current_build_of_a_branch() {
        assert_eq!(parse_app_info(APP_INFO, 258550, "public"), Some(25_681_086));
        assert_eq!(
            parse_app_info(APP_INFO, 258550, "staging"),
            Some(25_681_182)
        );
        assert_eq!(parse_app_info(APP_INFO, 258550, "aux02"), None);
        assert_eq!(steam_branch("release"), "public");
    }

    #[test]
    fn reads_the_installed_build_from_the_manifest() {
        let folder = std::env::temp_dir().join("crustation-steam-manifest");
        std::fs::create_dir_all(folder.join("steamapps")).unwrap();
        std::fs::write(
            folder.join("steamapps").join("appmanifest_258550.acf"),
            "\"AppState\"\n{\n\t\"appid\"\t\t\"258550\"\n\t\"buildid\"\t\t\"25681086\"\n}\n",
        )
        .unwrap();
        assert_eq!(installed_build(&folder, 258550), Some(25_681_086));
        assert_eq!(installed_build(&folder, 1), None);
        std::fs::remove_dir_all(&folder).ok();
    }

    #[tokio::test]
    async fn colour_codes_are_taken_out() {
        let (sender, mut lines) = unbounded_channel();
        let input: &[u8] = b"\x1b[0mWaiting for user info...\x1b[0mOK\n";
        split_lines(input, sender).await;
        assert_eq!(
            lines.recv().await.as_deref(),
            Some("Waiting for user info...OK")
        );
    }

    #[tokio::test]
    async fn carriage_returns_end_a_line_like_newlines_do() {
        let (sender, mut lines) = unbounded_channel();
        let input: &[u8] = b"first\rsecond\r\nthird";
        split_lines(input, sender).await;

        let mut got = Vec::new();
        while let Some(line) = lines.recv().await {
            got.push(line);
        }
        assert_eq!(got, ["first", "second", "third"]);
    }
}
