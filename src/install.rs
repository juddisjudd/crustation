use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use chrono::Utc;
use futures::StreamExt;
use serde_json::json;
use sha2::Digest;
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

use crate::events::Topic;
use crate::properties::{self, Properties};
use crate::providers::{self, Artifact, JavaHint, Layout};
use crate::state::AppState;
use crate::supervisor::Flag;

/// Where the files for a new server come from.
pub enum Source {
    Provider {
        provider: String,
        version: String,
    },
    /// A download the operator pasted, for anything the providers cannot reach.
    Url {
        url: String,
        executable: Option<String>,
    },
    /// An archive already uploaded to the panel.
    Zip {
        archive: PathBuf,
        internal_path: String,
        executable: Option<String>,
    },
    /// A server folder that already exists on this host.
    Folder {
        path: PathBuf,
        executable: Option<String>,
    },
}

pub struct Job {
    pub id: Uuid,
    pub name: String,
    pub kind: String,
    pub directory: PathBuf,
    pub source: Source,
    pub port: i64,
    pub java_binary: Option<String>,
    pub min_memory_mb: i64,
    pub max_memory_mb: i64,
    pub java_flags: String,
    pub agree_to_eula: bool,
    /// Checked against the catalogue before it got here, in a stable order.
    pub properties: Vec<(String, String)>,
}

/// What the install settled on, written back to the server row.
struct Installed {
    executable: String,
    command: String,
    provider: Option<String>,
    provider_version: Option<String>,
    java_binary: Option<String>,
    java_flags: String,
}

/// What the files turned out to be, before the JVM is chosen.
struct Placed {
    executable: String,
    layout: Layout,
    provider: Option<String>,
    provider_version: Option<String>,
    java: JavaHint,
}

/// The JVM an install settled on, and the flags it will run with.
#[derive(Default)]
struct Runtime {
    /// None leaves the command saying `java`, resolved on the path at start.
    binary: Option<String>,
    flags: Vec<String>,
}

/// Runs the install in the background. The caller has already inserted the row.
pub fn spawn(state: AppState, job: Job) {
    tokio::spawn(async move {
        let id = job.id;
        let name = job.name.clone();
        state.supervisor.set_flag(id, Flag::Installing, true).await;

        match run(&state, &job).await {
            Ok(installed) => match save(&state, &job, &installed).await {
                Ok(()) => {
                    state.supervisor.set_flag(id, Flag::Installing, false).await;
                    progress(&state, id, "done", Some(100.0), "Ready to start.");
                    log(&state, id, format!("{name} is ready to start.")).await;
                    state
                        .events
                        .publish(Topic::Servers, "created", json!({ "server_id": id }));
                }
                Err(error) => fail(&state, id, &name, &error).await,
            },
            Err(error) => fail(&state, id, &name, &error).await,
        }
    });
}

async fn fail(state: &AppState, id: Uuid, name: &str, error: &anyhow::Error) {
    tracing::warn!(%error, server = %id, "install failed");
    state.supervisor.set_flag(id, Flag::Installing, false).await;
    // The whole chain, so the console says "running java: program not found"
    // rather than leaving the operator with "running java".
    let reason = format!("{error:#}");
    progress(state, id, "failed", None, &reason);
    log(state, id, format!("Install failed: {reason}")).await;
    state
        .events
        .notify("error", format!("Could not set up {name}: {reason}"));
    // The row stays, so the operator can read the console and retry or delete it.
    state
        .events
        .publish(Topic::Servers, "created", json!({ "server_id": id }));
}

fn progress(state: &AppState, id: Uuid, phase: &str, percent: Option<f64>, message: &str) {
    state.events.publish(
        Topic::Server(id),
        "install",
        json!({
            "server_id": id,
            "phase": phase,
            "percent": percent,
            "message": message,
        }),
    );
}

/// Install notes go to the same console the operator watches once it is running.
async fn log(state: &AppState, id: Uuid, text: String) {
    state.supervisor.push_console(id, "install", text).await;
}

async fn run(state: &AppState, job: &Job) -> Result<Installed> {
    tokio::fs::create_dir_all(&job.directory)
        .await
        .with_context(|| format!("creating {}", job.directory.display()))?;

    let placed = match &job.source {
        Source::Provider { provider, version } => {
            progress(state, job.id, "resolving", None, "Looking up the download.");
            log(state, job.id, format!("Resolving {provider} {version}.")).await;
            let artifact = providers::resolve(&state.http, provider, version)
                .await
                .context("finding the download")?;
            Placed {
                executable: place(state, job, &artifact).await?,
                layout: artifact.layout,
                provider: Some(provider.clone()),
                provider_version: Some(artifact.version),
                java: artifact.java,
            }
        }
        Source::Url { url, executable } => {
            let layout = match strip_query(url).ends_with(".zip") {
                true => Layout::Zip {
                    executable: executable
                        .clone()
                        .unwrap_or_else(|| providers::bedrock_executable().to_string()),
                },
                false => Layout::Jar,
            };
            let artifact = Artifact {
                url: url.clone(),
                file_name: file_name_in(url),
                sha1: None,
                sha256: None,
                layout,
                version: String::new(),
                java: JavaHint::default(),
            };
            Placed {
                executable: place(state, job, &artifact).await?,
                layout: artifact.layout,
                provider: None,
                provider_version: None,
                java: artifact.java,
            }
        }
        Source::Zip {
            archive,
            internal_path,
            executable,
        } => {
            progress(state, job.id, "extracting", None, "Unpacking the archive.");
            log(state, job.id, "Unpacking the archive.".into()).await;
            unpack(archive, &job.directory, internal_path).await?;
            // The upload exists only for this import.
            tokio::fs::remove_file(archive).await.ok();
            let found = settle_executable(job, executable.clone()).await?;
            Placed {
                layout: imported_layout(job, &found),
                executable: found,
                provider: None,
                provider_version: None,
                java: JavaHint::default(),
            }
        }
        Source::Folder { path, executable } => {
            progress(state, job.id, "copying", None, "Copying the server folder.");
            log(state, job.id, format!("Copying {}.", path.display())).await;
            copy_into(path, &job.directory).await?;
            let found = settle_executable(job, executable.clone()).await?;
            Placed {
                layout: imported_layout(job, &found),
                executable: found,
                provider: None,
                provider_version: None,
                java: JavaHint::default(),
            }
        }
    };

    let runtime = choose_runtime(state, job, &placed.java).await;

    progress(
        state,
        job.id,
        "configuring",
        Some(95.0),
        "Writing settings.",
    );
    configure(state, job).await?;

    Ok(Installed {
        command: start_command(job, &runtime, &placed.executable, &placed.layout),
        executable: placed.executable,
        provider: placed.provider,
        provider_version: placed.provider_version,
        java_binary: runtime.binary,
        java_flags: shell_words::join(&runtime.flags),
    })
}

/// Aikar's flags: the G1 tuning the Minecraft community settled on, and what Paper
/// publishes for itself. Used where a project does not name its own.
const G1_FLAGS: [&str; 18] = [
    "-XX:+AlwaysPreTouch",
    "-XX:+DisableExplicitGC",
    "-XX:+ParallelRefProcEnabled",
    "-XX:+PerfDisableSharedMem",
    "-XX:+UnlockExperimentalVMOptions",
    "-XX:+UseG1GC",
    "-XX:G1HeapRegionSize=8M",
    "-XX:G1HeapWastePercent=5",
    "-XX:G1MaxNewSizePercent=40",
    "-XX:G1MixedGCCountTarget=4",
    "-XX:G1MixedGCLiveThresholdPercent=90",
    "-XX:G1NewSizePercent=30",
    "-XX:G1RSetUpdatingPauseTimePercent=5",
    "-XX:G1ReservePercent=20",
    "-XX:InitiatingHeapOccupancyPercent=15",
    "-XX:MaxGCPauseMillis=200",
    "-XX:MaxTenuringThreshold=1",
    "-XX:SurvivorRatio=32",
];

/// Aikar's guidance raises the region and nursery sizes once the heap is this big.
const LARGE_HEAP_MB: i64 = 12 * 1024;

/// Decides what the server runs on. Anything the operator asked for wins.
async fn choose_runtime(state: &AppState, job: &Job, hint: &JavaHint) -> Runtime {
    if job.kind != "minecraft_java" {
        return Runtime::default();
    }

    let flags = match job.java_flags.trim().is_empty() {
        // Nobody asked for anything, so run it the way the project would.
        true => {
            let published = match hint.flags.is_empty() {
                true => G1_FLAGS.iter().map(|flag| flag.to_string()).collect(),
                false => hint.flags.clone(),
            };
            for_heap(&published, job.max_memory_mb)
        }
        false => shell_words::split(&job.java_flags).unwrap_or_default(),
    };

    let binary = match &job.java_binary {
        Some(chosen) if !chosen.trim().is_empty() => Some(chosen.clone()),
        _ => pick_java(state, job, hint).await,
    };

    Runtime { binary, flags }
}

fn for_heap(flags: &[String], max_memory_mb: i64) -> Vec<String> {
    if max_memory_mb < LARGE_HEAP_MB {
        return flags.to_vec();
    }
    flags
        .iter()
        .map(|flag| match flag.split_once('=').map(|(key, _)| key) {
            Some("-XX:G1NewSizePercent") => "-XX:G1NewSizePercent=40".to_string(),
            Some("-XX:G1MaxNewSizePercent") => "-XX:G1MaxNewSizePercent=50".to_string(),
            Some("-XX:G1HeapRegionSize") => "-XX:G1HeapRegionSize=16M".to_string(),
            Some("-XX:G1ReservePercent") => "-XX:G1ReservePercent=15".to_string(),
            Some("-XX:InitiatingHeapOccupancyPercent") => {
                "-XX:InitiatingHeapOccupancyPercent=20".to_string()
            }
            _ => flag.clone(),
        })
        .collect()
}

/// Settles which JVM the server runs on. Knowing nothing about the version leaves
/// the command saying `java`, resolved on the path at start.
async fn pick_java(state: &AppState, job: &Job, hint: &JavaHint) -> Option<String> {
    let minimum = hint.minimum?;
    // discover() already hands them back newest first.
    let runtimes = crate::java::discover().await;

    match best_java(&runtimes, minimum, hint.any_newer) {
        Some(found) if found.major.is_some_and(|major| major >= minimum) => {
            log(
                state,
                job.id,
                format!("Running on Java {}.", found.major.unwrap_or(minimum)),
            )
            .await;
            Some(found.path.clone())
        }
        Some(newest) => {
            let have = match newest.major {
                Some(major) => format!("Java {major}"),
                None => newest.version.clone(),
            };
            log(
                state,
                job.id,
                format!(
                    "This version wants Java {minimum}, but the newest here is {have}. \
                     It may not start."
                ),
            )
            .await;
            Some(newest.path.clone())
        }
        None => {
            log(
                state,
                job.id,
                format!("No Java found on this host. This version needs Java {minimum}."),
            )
            .await;
            None
        }
    }
}

/// Where the project states a floor, take the quickest JVM above it. Where it names
/// the runtime a release was built for, stay as close to that as the host allows:
/// Minecraft 1.16 asks for Java 8 and does not survive Java 25. When nothing
/// qualifies, the newest installed is the best of a bad lot.
fn best_java(
    runtimes: &[crate::java::Runtime],
    minimum: u32,
    any_newer: bool,
) -> Option<&crate::java::Runtime> {
    let eligible = runtimes
        .iter()
        .filter(|runtime| runtime.major.is_some_and(|major| major >= minimum));

    match any_newer {
        true => eligible.max_by_key(|runtime| runtime.major),
        false => eligible.min_by_key(|runtime| runtime.major),
    }
    .or_else(|| runtimes.first())
}

/// An import is a jar when it is one, and a plain executable otherwise.
fn imported_layout(job: &Job, executable: &str) -> Layout {
    match job.kind == "minecraft_java" && executable.ends_with(".jar") {
        true => Layout::Jar,
        false => Layout::Zip {
            executable: executable.to_string(),
        },
    }
}

/// Fetches the artifact and leaves the folder ready to start. Returns the file the
/// server is launched from.
async fn place(state: &AppState, job: &Job, artifact: &Artifact) -> Result<String> {
    match &artifact.layout {
        Layout::Jar => {
            let target = job.directory.join(&artifact.file_name);
            download(state, job.id, artifact, &target).await?;
            Ok(artifact.file_name.clone())
        }
        Layout::Zip { executable } => {
            let archive = job.directory.join(&artifact.file_name);
            download(state, job.id, artifact, &archive).await?;

            progress(state, job.id, "extracting", Some(80.0), "Unpacking.");
            log(state, job.id, "Unpacking the download.".into()).await;
            unpack(&archive, &job.directory, "").await?;
            tokio::fs::remove_file(&archive).await.ok();

            make_executable(&job.directory.join(executable)).await;
            Ok(executable.clone())
        }
        Layout::Installer { neoforge_version } => {
            let installer = job.directory.join(&artifact.file_name);
            download(state, job.id, artifact, &installer).await?;

            progress(
                state,
                job.id,
                "installing",
                Some(80.0),
                "Running the NeoForge installer.",
            );
            log(state, job.id, "Running the NeoForge installer.".into()).await;
            run_installer(state, job, &artifact.file_name).await?;
            tokio::fs::remove_file(&installer).await.ok();

            neoforge_args(&job.directory, neoforge_version)
                .await
                .context("the NeoForge installer left no start arguments behind")
        }
    }
}

async fn download(state: &AppState, id: Uuid, artifact: &Artifact, target: &Path) -> Result<()> {
    progress(
        state,
        id,
        "downloading",
        Some(0.0),
        "Starting the download.",
    );
    log(state, id, format!("Downloading {}", artifact.url)).await;

    let response = state
        .http
        .get(&artifact.url)
        .send()
        .await
        .with_context(|| format!("fetching {}", artifact.url))?
        .error_for_status()
        .with_context(|| format!("fetching {}", artifact.url))?;

    let total = response.content_length();
    let mut file = tokio::fs::File::create(target)
        .await
        .with_context(|| format!("creating {}", target.display()))?;

    let mut sha1 = sha1::Sha1::new();
    let mut sha256 = sha2::Sha256::new();
    let mut done: u64 = 0;
    let mut last = Instant::now();
    let mut stream = response.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("the download was cut short")?;
        if artifact.sha1.is_some() {
            sha1.update(&chunk);
        }
        if artifact.sha256.is_some() {
            sha256.update(&chunk);
        }
        file.write_all(&chunk).await?;
        done += chunk.len() as u64;

        if last.elapsed() >= Duration::from_millis(400) {
            last = Instant::now();
            // Downloading is most of the wait, so it owns most of the bar.
            let percent = total.map(|total| done as f64 / total.max(1) as f64 * 75.0);
            progress(state, id, "downloading", percent, &transferred(done, total));
        }
    }
    file.flush().await?;
    drop(file);

    if let Some(expected) = &artifact.sha1 {
        verify("SHA-1", expected, &hex(&sha1.finalize()))?;
    }
    if let Some(expected) = &artifact.sha256 {
        verify("SHA-256", expected, &hex(&sha256.finalize()))?;
    }

    progress(state, id, "downloading", Some(75.0), "Download finished.");
    log(state, id, format!("Downloaded {}", size(done))).await;
    Ok(())
}

fn verify(algorithm: &str, expected: &str, actual: &str) -> Result<()> {
    if !expected.eq_ignore_ascii_case(actual) {
        bail!("the download did not match its {algorithm} checksum");
    }
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KiB", "MiB", "GiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

fn transferred(done: u64, total: Option<u64>) -> String {
    match total {
        Some(total) => format!("{} of {}", size(done), size(total)),
        None => size(done),
    }
}

/// NeoForge ships an installer rather than a server, so it has to be run once.
async fn run_installer(state: &AppState, job: &Job, installer: &str) -> Result<()> {
    let java = job.java_binary.clone().unwrap_or_else(|| "java".into());
    let output = tokio::process::Command::new(&java)
        .args(["-jar", installer, "--installServer"])
        .current_dir(&job.directory)
        .output()
        .await
        .with_context(|| format!("running {java}"))?;

    for line in String::from_utf8_lossy(&output.stdout).lines().take(200) {
        log(state, job.id, line.to_string()).await;
    }
    if !output.status.success() {
        for line in String::from_utf8_lossy(&output.stderr).lines().take(50) {
            log(state, job.id, line.to_string()).await;
        }
        bail!("the NeoForge installer failed");
    }
    Ok(())
}

/// The arguments file the installer writes, which the start command points at.
async fn neoforge_args(directory: &Path, version: &str) -> Option<String> {
    let name = if cfg!(windows) {
        "win_args.txt"
    } else {
        "unix_args.txt"
    };
    let relative = format!("libraries/net/neoforged/neoforge/{version}/{name}");
    tokio::fs::metadata(directory.join(&relative))
        .await
        .ok()
        .map(|_| relative)
}

fn start_command(job: &Job, runtime: &Runtime, executable: &str, layout: &Layout) -> String {
    if job.kind != "minecraft_java" {
        // Bedrock is its own binary, and Windows resolves it against the panel's
        // working directory rather than the server's, so give it the full path.
        return shell_words::quote(&job.directory.join(executable).to_string_lossy()).into_owned();
    }

    let java = runtime.binary.clone().unwrap_or_else(|| "java".into());
    let mut parts = vec![shell_words::quote(&java).into_owned()];
    parts.push(format!("-Xms{}M", job.min_memory_mb));
    parts.push(format!("-Xmx{}M", job.max_memory_mb));
    parts.extend(runtime.flags.iter().cloned());

    match layout {
        Layout::Installer { .. } => parts.push(format!("@{executable}")),
        _ => {
            parts.push("-jar".into());
            parts.push(shell_words::quote(executable).into_owned());
        }
    }
    parts.push("nogui".into());
    parts.join(" ")
}

/// Accepts the EULA, applies the settings that were asked for, and points the
/// server at the port it was created with.
async fn configure(state: &AppState, job: &Job) -> Result<()> {
    if job.kind == "minecraft_java" && job.agree_to_eula {
        let eula = job.directory.join("eula.txt");
        tokio::fs::write(&eula, "# Accepted through Crustation.\neula=true\n")
            .await
            .with_context(|| format!("writing {}", eula.display()))?;
        log(state, job.id, "Accepted the Minecraft EULA.".into()).await;
    }

    let path = properties::path_in(&job.directory);
    let mut file = match Properties::load_if_present(&path).await? {
        Some(file) => file,
        None => Properties::empty(&path),
    };

    for (key, value) in &job.properties {
        file.set(key, value);
    }
    // The panel owns the port, so it is written last and wins either way.
    file.set("server-port", &job.port.to_string());
    if job.kind == "minecraft_bedrock" {
        // Mojang's own default keeps IPv6 one port above IPv4.
        file.set("server-portv6", &(job.port + 1).to_string());
    }
    file.save().await?;

    if !job.properties.is_empty() {
        let names: Vec<&str> = job.properties.iter().map(|(key, _)| key.as_str()).collect();
        log(state, job.id, format!("Set {}.", names.join(", "))).await;
    }
    Ok(())
}

async fn save(state: &AppState, job: &Job, installed: &Installed) -> Result<()> {
    sqlx::query(
        "UPDATE servers SET executable = ?, command = ?, provider = ?, provider_version = ?,
            java_binary = ?, java_flags = ?, updated_at = ? WHERE id = ?",
    )
    .bind(&installed.executable)
    .bind(&installed.command)
    .bind(&installed.provider)
    .bind(&installed.provider_version)
    .bind(&installed.java_binary)
    .bind(&installed.java_flags)
    .bind(Utc::now().to_rfc3339())
    .bind(job.id.to_string())
    .execute(&state.db)
    .await?;
    Ok(())
}

/// Uses the executable the operator named, or works it out from what was imported.
async fn settle_executable(job: &Job, given: Option<String>) -> Result<String> {
    if let Some(name) = given.filter(|name| !name.trim().is_empty()) {
        let path = job.directory.join(&name);
        if !tokio::fs::try_exists(&path).await.unwrap_or(false) {
            bail!("{name} is not in the imported files");
        }
        make_executable(&path).await;
        return Ok(name);
    }

    if job.kind == "minecraft_bedrock" {
        let name = providers::bedrock_executable();
        let path = job.directory.join(name);
        if !tokio::fs::try_exists(&path).await.unwrap_or(false) {
            bail!("no {name} in the imported files");
        }
        make_executable(&path).await;
        return Ok(name.to_string());
    }

    find_jar(&job.directory)
        .await?
        .ok_or_else(|| anyhow!("no server jar in the imported files; name the one to use"))
}

/// Prefers the conventional name, then the largest jar that is not an installer.
async fn find_jar(directory: &Path) -> Result<Option<String>> {
    if tokio::fs::try_exists(directory.join("server.jar"))
        .await
        .unwrap_or(false)
    {
        return Ok(Some("server.jar".into()));
    }

    let mut entries = tokio::fs::read_dir(directory).await?;
    let mut best: Option<(u64, String)> = None;
    while let Some(entry) = entries.next_entry().await? {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.ends_with(".jar") || name.contains("installer") {
            continue;
        }
        let size = entry.metadata().await.map(|data| data.len()).unwrap_or(0);
        if best.as_ref().is_none_or(|(largest, _)| size > *largest) {
            best = Some((size, name));
        }
    }
    Ok(best.map(|(_, name)| name))
}

#[cfg(unix)]
async fn make_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(metadata) = tokio::fs::metadata(path).await {
        let mut permissions = metadata.permissions();
        permissions.set_mode(permissions.mode() | 0o755);
        tokio::fs::set_permissions(path, permissions).await.ok();
    }
}

#[cfg(not(unix))]
async fn make_executable(_path: &Path) {}

async fn unpack(archive: &Path, into: &Path, internal_path: &str) -> Result<()> {
    let archive = archive.to_path_buf();
    let into = into.to_path_buf();
    let internal = internal_path.trim_matches('/').to_string();
    tokio::task::spawn_blocking(move || extract(&archive, &into, &internal))
        .await
        .context("unpacking the archive")?
}

fn extract(archive: &Path, into: &Path, internal: &str) -> Result<()> {
    let file =
        std::fs::File::open(archive).with_context(|| format!("opening {}", archive.display()))?;
    let mut zip = zip::ZipArchive::new(std::io::BufReader::new(file))
        .with_context(|| format!("reading {}", archive.display()))?;

    for index in 0..zip.len() {
        let mut entry = zip.by_index(index)?;
        // enclosed_name refuses absolute paths and anything climbing out with '..'.
        let Some(path) = entry.enclosed_name() else {
            continue;
        };
        let relative = match internal.is_empty() {
            true => path,
            false => match path.strip_prefix(internal) {
                Ok(rest) => rest.to_path_buf(),
                Err(_) => continue,
            },
        };
        if relative.as_os_str().is_empty() {
            continue;
        }

        let target = into.join(&relative);
        if entry.is_dir() {
            std::fs::create_dir_all(&target)?;
            continue;
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut out = std::fs::File::create(&target)
            .with_context(|| format!("writing {}", target.display()))?;
        std::io::copy(&mut entry, &mut out)?;

        #[cfg(unix)]
        if let Some(mode) = entry.unix_mode() {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&target, std::fs::Permissions::from_mode(mode)).ok();
        }
    }
    Ok(())
}

async fn copy_into(from: &Path, into: &Path) -> Result<()> {
    let from = from.to_path_buf();
    let into = into.to_path_buf();
    tokio::task::spawn_blocking(move || copy_tree(&from, &into))
        .await
        .context("copying the server folder")?
}

fn copy_tree(from: &Path, into: &Path) -> Result<()> {
    for entry in walkdir::WalkDir::new(from).follow_links(false) {
        let entry = entry?;
        let relative = entry.path().strip_prefix(from)?;
        if relative.as_os_str().is_empty() {
            continue;
        }
        let target = into.join(relative);
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&target)?;
        } else if entry.file_type().is_file() {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(entry.path(), &target)
                .with_context(|| format!("copying {}", entry.path().display()))?;
        }
    }
    Ok(())
}

fn strip_query(url: &str) -> &str {
    url.split('?').next().unwrap_or(url)
}

fn file_name_in(url: &str) -> String {
    strip_query(url)
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or("download")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(kind: &str, java_flags: &str) -> Job {
        Job {
            id: Uuid::nil(),
            name: "Test".into(),
            kind: kind.into(),
            directory: PathBuf::from("/servers/test"),
            source: Source::Provider {
                provider: "paper".into(),
                version: "1.21.11".into(),
            },
            port: 25565,
            java_binary: None,
            min_memory_mb: 1024,
            max_memory_mb: 4096,
            java_flags: java_flags.into(),
            agree_to_eula: true,
            properties: Vec::new(),
        }
    }

    fn runtime(binary: Option<&str>, flags: &[&str]) -> Runtime {
        Runtime {
            binary: binary.map(str::to_string),
            flags: flags.iter().map(|flag| flag.to_string()).collect(),
        }
    }

    #[test]
    fn builds_a_java_command_with_memory_and_flags() {
        let command = start_command(
            &job("minecraft_java", ""),
            &runtime(None, &["-XX:+UseG1GC"]),
            "paper.jar",
            &Layout::Jar,
        );
        assert_eq!(
            command,
            "java -Xms1024M -Xmx4096M -XX:+UseG1GC -jar paper.jar nogui"
        );
    }

    #[test]
    fn runs_on_the_java_that_was_picked() {
        let command = start_command(
            &job("minecraft_java", ""),
            &runtime(Some("/usr/lib/jvm/java-25-openjdk-amd64/bin/java"), &[]),
            "paper.jar",
            &Layout::Jar,
        );
        assert!(command.starts_with("/usr/lib/jvm/java-25-openjdk-amd64/bin/java -Xms1024M"));
    }

    #[test]
    fn points_neoforge_at_its_arguments_file() {
        let layout = Layout::Installer {
            neoforge_version: "21.1.251".into(),
        };
        let args = "libraries/net/neoforged/neoforge/21.1.251/unix_args.txt";
        let command = start_command(
            &job("minecraft_java", ""),
            &Runtime::default(),
            args,
            &layout,
        );
        assert!(command.contains(&format!("@{args}")));
        assert!(!command.contains("-jar"));
    }

    #[test]
    fn bedrock_runs_its_own_binary_without_java() {
        let command = start_command(
            &job("minecraft_bedrock", ""),
            &Runtime::default(),
            "bedrock_server",
            &Layout::Jar,
        );
        assert!(command.contains("bedrock_server"));
        assert!(!command.contains("java"));
    }

    /// The supervisor splits the stored command with the same crate, and a Windows
    /// path is full of backslashes it would otherwise read as escapes.
    #[test]
    fn a_bedrock_path_survives_being_split_again() {
        let mut job = job("minecraft_bedrock", "");
        job.directory = PathBuf::from(r"C:\Program Files\servers\abc");
        let command = start_command(
            &job,
            &Runtime::default(),
            "bedrock_server.exe",
            &Layout::Jar,
        );

        // join picks the separator, so ask it rather than spelling one out: the
        // backslashes and the space are what this is really about.
        let expected = job.directory.join("bedrock_server.exe");
        let parts = shell_words::split(&command).expect("the command parses");
        assert_eq!(parts, [expected.to_string_lossy()]);
    }

    /// The flags are stored as one string and split again on the next start, so the
    /// whole set has to survive the round trip intact.
    #[test]
    fn the_recommended_flags_survive_being_stored_and_split() {
        let flags: Vec<String> = G1_FLAGS.iter().map(|flag| flag.to_string()).collect();
        let stored = shell_words::join(&flags);
        assert_eq!(shell_words::split(&stored).expect("parses"), flags);

        let command = start_command(
            &job("minecraft_java", ""),
            &Runtime {
                binary: None,
                flags,
            },
            "paper.jar",
            &Layout::Jar,
        );
        let parts = shell_words::split(&command).expect("the command parses");
        assert_eq!(parts.first().map(String::as_str), Some("java"));
        assert!(parts.contains(&"-XX:+UseG1GC".to_string()));
        assert_eq!(parts.last().map(String::as_str), Some("nogui"));
    }

    fn installed(major: u32) -> crate::java::Runtime {
        crate::java::Runtime {
            path: format!("/usr/lib/jvm/java-{major}/bin/java"),
            version: format!("openjdk version \"{major}.0.1\""),
            major: Some(major),
        }
    }

    /// Paper states a floor it supports anything above, so the quickest JVM wins.
    #[test]
    fn a_stated_floor_takes_the_newest_java_installed() {
        let runtimes = [installed(25), installed(21), installed(17), installed(8)];

        assert_eq!(best_java(&runtimes, 25, true).unwrap().major, Some(25));
        assert_eq!(best_java(&runtimes, 21, true).unwrap().major, Some(25));
    }

    /// Mojang names the runtime a release was built for, and 1.16 does not survive
    /// Java 25, so the closest match wins instead.
    #[test]
    fn a_named_runtime_stays_as_close_as_it_can() {
        let runtimes = [installed(25), installed(21), installed(17), installed(8)];

        assert_eq!(best_java(&runtimes, 8, false).unwrap().major, Some(8));
        assert_eq!(best_java(&runtimes, 17, false).unwrap().major, Some(17));
        assert_eq!(best_java(&runtimes, 25, false).unwrap().major, Some(25));
    }

    #[test]
    fn falls_back_to_the_newest_when_nothing_is_new_enough() {
        let runtimes = [installed(21), installed(17)];
        assert_eq!(best_java(&runtimes, 25, true).unwrap().major, Some(21));
        assert_eq!(best_java(&runtimes, 25, false).unwrap().major, Some(21));
    }

    #[test]
    fn picks_nothing_when_no_java_is_installed() {
        assert!(best_java(&[], 21, true).is_none());
    }

    #[test]
    fn small_heaps_keep_the_published_flags() {
        let flags: Vec<String> = G1_FLAGS.iter().map(|flag| flag.to_string()).collect();
        assert_eq!(for_heap(&flags, 4096), flags);
    }

    #[test]
    fn large_heaps_get_the_bigger_regions_aikar_asks_for() {
        let flags: Vec<String> = G1_FLAGS.iter().map(|flag| flag.to_string()).collect();
        let tuned = for_heap(&flags, 16 * 1024);

        assert!(tuned.contains(&"-XX:G1NewSizePercent=40".to_string()));
        assert!(tuned.contains(&"-XX:G1MaxNewSizePercent=50".to_string()));
        assert!(tuned.contains(&"-XX:G1HeapRegionSize=16M".to_string()));
        assert!(tuned.contains(&"-XX:G1ReservePercent=15".to_string()));
        assert!(tuned.contains(&"-XX:InitiatingHeapOccupancyPercent=20".to_string()));
        // Everything untouched stays put, and nothing is added or dropped.
        assert_eq!(tuned.len(), flags.len());
        assert!(tuned.contains(&"-XX:+UseG1GC".to_string()));
        assert!(tuned.contains(&"-XX:MaxGCPauseMillis=200".to_string()));
    }

    #[test]
    fn reads_a_file_name_out_of_a_url() {
        assert_eq!(
            file_name_in("https://host.test/a/b/server.zip?x=1"),
            "server.zip"
        );
        assert_eq!(file_name_in("https://host.test/"), "download");
    }
}
