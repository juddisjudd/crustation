use std::collections::BTreeMap;
use std::path::PathBuf;

use axum::extract::{DefaultBodyLimit, Path, Query, State};
use axum::response::IntoResponse;
use axum::{
    Json, Router,
    routing::{get, post},
};
use chrono::{DateTime, Utc};
use futures::StreamExt;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::FromRow;
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

use crate::auth::Identity;
use crate::error::{ApiError, ApiResult, Created, Done, Ok as OkJson};
use crate::install::{self, Job};
use crate::perms::{Global, Server as ServerPerm};
use crate::properties::{self, Properties};
use crate::providers;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(list).post(create))
        .route(
            "/import/upload",
            post(upload).layer(DefaultBodyLimit::disable()),
        )
        .route("/import/{upload_id}/entries", get(entries))
        .route("/stats", get(recent_stats))
        .route("/{id}", get(detail).patch(update).delete(remove))
        .route("/{id}/action", post(action))
        .route("/{id}/command", post(command))
        .route("/{id}/console", get(console))
        .route("/{id}/rcon", get(rcon_status).post(rcon_enable))
        .merge(crate::api::properties::server_routes())
        .merge(crate::api::files::routes())
        .merge(crate::api::macros::routes())
        .merge(crate::api::bridge::routes())
        .merge(crate::api::packs::routes())
        .merge(crate::api::players::routes())
}

#[derive(Debug, Clone, FromRow)]
pub struct ServerRow {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub directory: String,
    pub executable: String,
    pub command: String,
    pub java_binary: Option<String>,
    pub min_memory_mb: i64,
    pub max_memory_mb: i64,
    pub java_flags: String,
    pub host: String,
    pub port: i64,
    pub autostart: bool,
    pub autostart_delay: i64,
    pub crash_detection: bool,
    pub stop_command: String,
    pub shutdown_timeout: i64,
    pub ignored_exits: String,
    pub count_players: bool,
    pub public_status: bool,
    pub log_path: String,
    pub provider: Option<String>,
    pub provider_version: Option<String>,
    pub created_at: String,
}

impl ServerRow {
    pub fn uuid(&self) -> Uuid {
        Uuid::parse_str(&self.id).unwrap_or_default()
    }
}

/// The shape docs/API.md promises for a server.
pub async fn server_json(
    state: &AppState,
    row: &ServerRow,
    permissions: Vec<&'static str>,
) -> Value {
    let id = row.uuid();
    let runtime = state.supervisor.runtime(id).await;
    let stats = latest_stats(state, &row.id).await;

    json!({
        "id": row.id,
        "name": row.name,
        "kind": row.kind,
        "created_at": row.created_at,
        "address": { "host": row.host, "port": row.port },
        "settings": {
            "autostart": row.autostart,
            "autostart_delay": row.autostart_delay,
            "crash_detection": row.crash_detection,
            "stop_command": row.stop_command,
            "shutdown_timeout": row.shutdown_timeout,
            "ignored_exits": row.ignored_exits,
            "count_players": row.count_players,
            "public_status": row.public_status,
            "log_path": row.log_path,
            "directory": row.directory,
            "executable": row.executable,
            "command": row.command,
            "java": {
                "binary": row.java_binary,
                "min_memory_mb": row.min_memory_mb,
                "max_memory_mb": row.max_memory_mb,
                "flags": row.java_flags,
            },
            "provider": row.provider,
            "provider_version": row.provider_version,
        },
        "state": runtime.state_str,
        "stats": stats,
        "flags": {
            "installing": runtime.installing,
            "updating": runtime.updating,
            "backing_up": runtime.backing_up,
            "crashed": runtime.state_str == "crashed",
            "last_backup_failed": false,
            "update_available": false,
        },
        "permissions": permissions,
    })
}

#[derive(FromRow)]
struct StatsRow {
    at: String,
    cpu_percent: f64,
    memory_bytes: i64,
    memory_percent: f64,
    players_online: Option<i64>,
    players_max: Option<i64>,
}

async fn latest_stats(state: &AppState, server_id: &str) -> Value {
    let row: Option<StatsRow> = sqlx::query_as(
        "SELECT at, cpu_percent, memory_bytes, memory_percent, players_online, players_max
         FROM server_stats WHERE server_id = ? ORDER BY at DESC LIMIT 1",
    )
    .bind(server_id)
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten();

    let id = Uuid::parse_str(server_id).unwrap_or_default();
    let runtime = state.supervisor.runtime(id).await;
    // What the server itself last said, which is fresher than the stored sample
    // and is there even for a server the panel did not start.
    let live = state.statuses.get(id).await;

    let mut stats = match row {
        Some(stats) => json!({
            "at": stats.at,
            "cpu_percent": stats.cpu_percent,
            "memory_bytes": stats.memory_bytes,
            "memory_percent": stats.memory_percent,
            "players_online": stats.players_online,
            "players_max": stats.players_max,
            "started_at": runtime.started_at,
        }),
        None => json!({
            "cpu_percent": 0.0,
            "memory_bytes": 0,
            "memory_percent": 0.0,
            "players_online": null,
            "players_max": null,
            "started_at": runtime.started_at,
        }),
    };

    stats["version"] = json!(live.as_ref().map(|status| status.version.clone()));
    stats["motd"] = json!(live.as_ref().map(|status| status.motd.clone()));
    stats["latency_ms"] = json!(live.as_ref().map(|status| status.latency_ms));
    if let Some(status) = &live {
        stats["players_online"] = json!(status.players_online);
        stats["players_max"] = json!(status.players_max);
    }
    stats
}

async fn list(identity: Identity, State(state): State<AppState>) -> ApiResult<impl IntoResponse> {
    let rows: Vec<ServerRow> = sqlx::query_as("SELECT * FROM servers ORDER BY name COLLATE NOCASE")
        .fetch_all(&state.db)
        .await?;

    let visible = identity
        .visible_servers(&state.db)
        .await
        .map_err(ApiError::Internal)?;

    let mut out = Vec::new();
    for row in rows {
        if let Some(allowed) = &visible
            && !allowed.contains(&row.id)
        {
            continue;
        }
        let granted = identity
            .server_permissions(&state.db, row.uuid())
            .await
            .map_err(ApiError::Internal)?;
        let names = granted.iter().map(|p| p.as_str()).collect();
        out.push(server_json(&state, &row, names).await);
    }
    Ok(OkJson(out))
}

#[derive(Deserialize)]
struct Recent {
    /// How far back to look. Half an hour by default, a day at most.
    minutes: Option<i64>,
    /// How many points to come back with, so the line is drawn from a fixed
    /// number of buckets whatever the sampling interval happens to be.
    points: Option<usize>,
}

#[derive(sqlx::FromRow)]
struct StatRow {
    server_id: String,
    at: String,
    cpu_percent: f64,
    memory_percent: f64,
}

/// Recent history for every server the caller can see, in one call, so the
/// overview can draw a line on each card without asking per server.
async fn recent_stats(
    identity: Identity,
    State(state): State<AppState>,
    Query(query): Query<Recent>,
) -> ApiResult<impl IntoResponse> {
    let minutes = query.minutes.unwrap_or(30).clamp(1, 60 * 24);
    let points = query.points.unwrap_or(32).clamp(4, 200);
    let since = Utc::now() - chrono::Duration::minutes(minutes);

    let visible = identity
        .visible_servers(&state.db)
        .await
        .map_err(ApiError::Internal)?;

    let rows: Vec<StatRow> = sqlx::query_as(
        "SELECT server_id, at, cpu_percent, memory_percent
         FROM server_stats WHERE at >= ? ORDER BY at ASC",
    )
    .bind(since.to_rfc3339())
    .fetch_all(&state.db)
    .await?;

    let keep = |id: &str| match &visible {
        Some(allowed) => allowed.iter().any(|one| one == id),
        None => true,
    };
    let rows = rows.into_iter().filter(|row| keep(&row.server_id));

    Ok(OkJson(json!({
        "since": since.to_rfc3339(),
        "minutes": minutes,
        "series": bucket(rows, since, minutes, points),
    })))
}

/// Averages the samples into a fixed number of buckets, so a card always draws
/// the same number of points whatever the sampler's interval is set to, and a
/// long window is not a long payload.
///
/// A bucket nothing was sampled in comes back null rather than nought, so a
/// stretch where the server was down reads as a gap in the line instead of a
/// floor it never actually sat at.
fn bucket(
    rows: impl Iterator<Item = StatRow>,
    since: DateTime<Utc>,
    minutes: i64,
    points: usize,
) -> BTreeMap<String, Vec<Value>> {
    let span = (minutes * 60) as f64 / points as f64;
    let mut series: BTreeMap<String, Vec<(f64, f64, i64)>> = BTreeMap::new();

    for row in rows {
        let Ok(at) = DateTime::parse_from_rfc3339(&row.at) else {
            continue;
        };
        let offset = (at.with_timezone(&Utc) - since).num_seconds();
        if offset < 0 {
            continue;
        }
        let at = ((offset as f64) / span) as usize;
        let at = at.min(points - 1);

        let line = series
            .entry(row.server_id.clone())
            .or_insert_with(|| vec![(0.0, 0.0, 0); points]);
        line[at].0 += row.cpu_percent;
        line[at].1 += row.memory_percent;
        line[at].2 += 1;
    }

    series
        .into_iter()
        .map(|(id, line)| {
            let line = line
                .into_iter()
                .map(|(cpu, memory, seen)| match seen {
                    0 => Value::Null,
                    seen => json!({
                        "cpu": cpu / seen as f64,
                        "memory_percent": memory / seen as f64,
                    }),
                })
                .collect();
            (id, line)
        })
        .collect()
}

#[derive(Deserialize)]
struct Create {
    name: String,
    host: Option<String>,
    port: Option<i64>,
    min_memory_mb: Option<i64>,
    max_memory_mb: Option<i64>,
    java_binary: Option<String>,
    #[serde(default)]
    java_flags: String,
    #[serde(default)]
    autostart: bool,
    #[serde(default)]
    agree_to_eula: bool,
    /// Entries from the server.properties catalogue for this kind of server.
    #[serde(default)]
    properties: std::collections::BTreeMap<String, Value>,
    source: SourceBody,
}

/// Turns the requested settings into checked pairs, refusing anything this kind
/// of server does not offer. The order is stable so the file is written the same
/// way every time.
fn checked_properties(
    kind: &str,
    requested: std::collections::BTreeMap<String, Value>,
) -> Result<Vec<(String, String)>, ApiError> {
    let mut out = Vec::new();
    for (key, value) in requested {
        let field = format!("properties.{key}");
        let Some(entry) = properties::known(kind, &key) else {
            return Err(ApiError::field(
                field,
                "This kind of server does not have that setting.",
            ));
        };
        let raw = match value {
            Value::Bool(flag) => flag.to_string(),
            Value::Number(number) => number.to_string(),
            Value::String(text) => text,
            _ => {
                return Err(ApiError::field(
                    field,
                    "Send text, a number, or true and false.",
                ));
            }
        };
        let checked =
            properties::check(entry, &raw).map_err(|message| ApiError::field(field, message))?;
        out.push((key, checked));
    }
    Ok(out)
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum SourceBody {
    Provider {
        provider: String,
        version: String,
    },
    Url {
        kind: String,
        url: String,
        executable: Option<String>,
    },
    Zip {
        kind: String,
        upload_id: String,
        #[serde(default)]
        internal_path: String,
        executable: Option<String>,
    },
    Folder {
        kind: String,
        path: String,
        executable: Option<String>,
    },
}

/// Inserts the row, then does the downloading in the background. The interface
/// follows along on the server's `install` events.
async fn create(
    identity: Identity,
    State(state): State<AppState>,
    Json(body): Json<Create>,
) -> ApiResult<impl IntoResponse> {
    identity.require_global(Global::CreateServer)?;

    let name = body.name.trim().to_string();
    if name.is_empty() {
        return Err(ApiError::field("name", "Give the server a name."));
    }

    let id = Uuid::new_v4();
    let directory = state.config.server_dir(&id);

    let (kind, source) = match body.source {
        SourceBody::Provider { provider, version } => {
            let info = providers::find(&provider)
                .ok_or_else(|| ApiError::field("source.provider", "No provider by that name."))?;
            if version.trim().is_empty() {
                return Err(ApiError::field("source.version", "Choose a version."));
            }
            (
                info.kind.to_string(),
                install::Source::Provider { provider, version },
            )
        }
        SourceBody::Url {
            kind,
            url,
            executable,
        } => {
            if !url.starts_with("http://") && !url.starts_with("https://") {
                return Err(ApiError::field(
                    "source.url",
                    "Paste an http or https link.",
                ));
            }
            (
                checked_kind(&kind)?,
                install::Source::Url { url, executable },
            )
        }
        SourceBody::Zip {
            kind,
            upload_id,
            internal_path,
            executable,
        } => {
            let archive = upload_path(&state, &upload_id)?;
            if !tokio::fs::try_exists(&archive).await.unwrap_or(false) {
                return Err(ApiError::field(
                    "source.upload_id",
                    "That upload has expired. Send the archive again.",
                ));
            }
            (
                checked_kind(&kind)?,
                install::Source::Zip {
                    archive,
                    internal_path,
                    executable,
                },
            )
        }
        SourceBody::Folder {
            kind,
            path,
            executable,
        } => {
            // Reading any folder on the host is more than CREATE_SERVER should grant.
            if !identity.is_admin() {
                return Err(ApiError::forbidden(
                    "Only an administrator can import a folder from this host.",
                ));
            }
            let path = PathBuf::from(path);
            let is_directory = tokio::fs::metadata(&path)
                .await
                .map(|data| data.is_dir())
                .unwrap_or(false);
            if !is_directory {
                return Err(ApiError::field(
                    "source.path",
                    "There is no folder at that path on this host.",
                ));
            }
            (
                checked_kind(&kind)?,
                install::Source::Folder { path, executable },
            )
        }
    };

    if !body.agree_to_eula {
        return Err(ApiError::field(
            "agree_to_eula",
            "Accept the Minecraft end user licence agreement to continue.",
        ));
    }

    let settings = checked_properties(&kind, body.properties)?;

    let port = body.port.unwrap_or_else(|| default_port(&kind));
    if !(1..=65535).contains(&port) {
        return Err(ApiError::field("port", "Pick a port between 1 and 65535."));
    }

    let min_memory_mb = body.min_memory_mb.unwrap_or(1024);
    let max_memory_mb = body.max_memory_mb.unwrap_or(4096);
    if min_memory_mb < 128 {
        return Err(ApiError::field(
            "min_memory_mb",
            "Give the server at least 128 MB.",
        ));
    }
    if min_memory_mb > max_memory_mb {
        return Err(ApiError::field(
            "min_memory_mb",
            "Minimum memory cannot exceed the maximum.",
        ));
    }

    let host = body.host.unwrap_or_else(|| "0.0.0.0".to_string());
    // Bedrock logs to its console only; Java keeps a log file worth reading.
    let log_path = match kind.as_str() {
        "minecraft_java" => "logs/latest.log",
        _ => "",
    };
    let now = Utc::now().to_rfc3339();

    sqlx::query(
        "INSERT INTO servers (id, name, kind, directory, java_binary, min_memory_mb,
             max_memory_mb, java_flags, host, port, autostart, log_path, created_by,
             created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(id.to_string())
    .bind(&name)
    .bind(&kind)
    .bind(directory.to_string_lossy().as_ref())
    .bind(&body.java_binary)
    .bind(min_memory_mb)
    .bind(max_memory_mb)
    .bind(&body.java_flags)
    .bind(&host)
    .bind(port)
    .bind(body.autostart)
    .bind(log_path)
    .bind(&identity.user.id)
    .bind(&now)
    .bind(&now)
    .execute(&state.db)
    .await?;

    install::spawn(
        state.clone(),
        Job {
            id,
            name: name.clone(),
            kind,
            directory,
            source,
            port,
            java_binary: body.java_binary,
            min_memory_mb,
            max_memory_mb,
            java_flags: body.java_flags,
            agree_to_eula: body.agree_to_eula,
            properties: settings,
        },
    );

    crate::api::audit(
        &state,
        Some(&identity.user),
        Some(id),
        "created a server",
        Some(&name),
    )
    .await;
    state.events.publish(
        crate::events::Topic::Servers,
        "created",
        json!({ "server_id": id }),
    );

    Ok(Created(json!({ "id": id })))
}

fn checked_kind(kind: &str) -> Result<String, ApiError> {
    match kind {
        "minecraft_java" | "minecraft_bedrock" => Ok(kind.to_string()),
        _ => Err(ApiError::field("source.kind", "Choose Java or Bedrock.")),
    }
}

fn default_port(kind: &str) -> i64 {
    match kind {
        "minecraft_bedrock" => 19132,
        _ => 25565,
    }
}

/// Where an uploaded archive waits until it is turned into a server. It sits on
/// the servers volume, which is the one with room for it.
fn uploads_dir(state: &AppState) -> PathBuf {
    state.config.paths.servers.join(".uploads")
}

/// Parsing the id as a UUID is also what keeps the name from escaping the folder.
fn upload_path(state: &AppState, upload_id: &str) -> Result<PathBuf, ApiError> {
    let id = Uuid::parse_str(upload_id).map_err(|_| ApiError::not_found("Upload"))?;
    Ok(uploads_dir(state).join(format!("{id}.zip")))
}

/// Past any real server archive, and still a bound on a runaway upload.
const MAX_UPLOAD_BYTES: u64 = 16 * 1024 * 1024 * 1024;

async fn upload(
    identity: Identity,
    State(state): State<AppState>,
    body: axum::body::Body,
) -> ApiResult<impl IntoResponse> {
    identity.require_global(Global::CreateServer)?;

    let directory = uploads_dir(&state);
    tokio::fs::create_dir_all(&directory).await?;
    sweep(&directory).await;

    let upload_id = Uuid::new_v4();
    let target = directory.join(format!("{upload_id}.zip"));
    let mut file = tokio::fs::File::create(&target).await?;

    let mut written: u64 = 0;
    let mut stream = body.into_data_stream();
    while let Some(chunk) = stream.next().await {
        let chunk =
            chunk.map_err(|error| ApiError::validation(format!("Upload failed: {error}")))?;
        written += chunk.len() as u64;
        if written > MAX_UPLOAD_BYTES {
            drop(file);
            tokio::fs::remove_file(&target).await.ok();
            return Err(ApiError::validation("That archive is too large to import."));
        }
        file.write_all(&chunk).await?;
    }
    file.flush().await?;
    drop(file);

    if written == 0 {
        tokio::fs::remove_file(&target).await.ok();
        return Err(ApiError::validation("The upload was empty."));
    }

    Ok(Created(
        json!({ "upload_id": upload_id, "size_bytes": written }),
    ))
}

/// Drops uploads nobody turned into a server.
async fn sweep(directory: &std::path::Path) {
    let Ok(mut entries) = tokio::fs::read_dir(directory).await else {
        return;
    };
    let cutoff = std::time::SystemTime::now() - std::time::Duration::from_secs(24 * 60 * 60);
    while let Ok(Some(entry)) = entries.next_entry().await {
        let stale = entry
            .metadata()
            .await
            .ok()
            .and_then(|data| data.modified().ok())
            .is_some_and(|modified| modified < cutoff);
        if stale {
            tokio::fs::remove_file(entry.path()).await.ok();
        }
    }
}

/// Folders inside an uploaded archive that look like a server, so the operator can
/// say which one to import.
async fn entries(
    identity: Identity,
    State(state): State<AppState>,
    Path(upload_id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    identity.require_global(Global::CreateServer)?;
    let archive = upload_path(&state, &upload_id)?;
    if !tokio::fs::try_exists(&archive).await.unwrap_or(false) {
        return Err(ApiError::not_found("Upload"));
    }

    let roots = tokio::task::spawn_blocking(move || read_roots(&archive))
        .await
        .map_err(|error| ApiError::Internal(error.into()))?
        .map_err(ApiError::Internal)?;
    Ok(OkJson(roots))
}

fn read_roots(archive: &std::path::Path) -> anyhow::Result<Vec<Value>> {
    use std::collections::BTreeMap;

    let file = std::fs::File::open(archive)?;
    let mut zip = zip::ZipArchive::new(std::io::BufReader::new(file))?;
    let mut roots: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for index in 0..zip.len() {
        let entry = zip.by_index(index)?;
        if entry.is_dir() {
            continue;
        }
        let Some(path) = entry.enclosed_name() else {
            continue;
        };
        let Some(name) = path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
        else {
            continue;
        };
        let tells_a_server = name.ends_with(".jar")
            || name == "server.properties"
            || name.starts_with("bedrock_server");
        if !tells_a_server {
            continue;
        }
        let parent = path
            .parent()
            .map(|parent| parent.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        roots.entry(parent).or_default().push(name);
    }

    Ok(roots
        .into_iter()
        .map(|(path, files)| json!({ "path": path, "files": files }))
        .collect())
}

pub async fn load(state: &AppState, id: Uuid) -> Result<ServerRow, ApiError> {
    sqlx::query_as("SELECT * FROM servers WHERE id = ?")
        .bind(id.to_string())
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| ApiError::not_found("Server"))
}

async fn detail(
    identity: Identity,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<impl IntoResponse> {
    let granted = identity
        .server_permissions(&state.db, id)
        .await
        .map_err(ApiError::Internal)?;
    if granted.is_empty() {
        return Err(ApiError::not_found("Server"));
    }
    let row = load(&state, id).await?;
    let names = granted.iter().map(|p| p.as_str()).collect();
    Ok(OkJson(server_json(&state, &row, names).await))
}

#[derive(Deserialize)]
struct Update {
    name: Option<String>,
    host: Option<String>,
    port: Option<i64>,
    autostart: Option<bool>,
    autostart_delay: Option<i64>,
    crash_detection: Option<bool>,
    stop_command: Option<String>,
    shutdown_timeout: Option<i64>,
    ignored_exits: Option<String>,
    count_players: Option<bool>,
    public_status: Option<bool>,
    log_path: Option<String>,
    executable: Option<String>,
    command: Option<String>,
    java_binary: Option<String>,
    min_memory_mb: Option<i64>,
    max_memory_mb: Option<i64>,
    java_flags: Option<String>,
}

async fn update(
    identity: Identity,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<Update>,
) -> ApiResult<impl IntoResponse> {
    identity
        .require_server(&state.db, id, ServerPerm::Config)
        .await?;
    let row = load(&state, id).await?;

    if let Some(name) = &body.name
        && name.trim().is_empty()
    {
        return Err(ApiError::field("name", "Give the server a name."));
    }
    if let (Some(min), Some(max)) = (body.min_memory_mb, body.max_memory_mb)
        && min > max
    {
        return Err(ApiError::field(
            "min_memory_mb",
            "Minimum memory cannot exceed the maximum.",
        ));
    }
    // Only an admin may change how the process is launched.
    if (body.command.is_some() || body.java_binary.is_some()) && !identity.is_admin() {
        return Err(ApiError::forbidden(
            "Only an administrator can change the start command.",
        ));
    }

    let name = body.name.unwrap_or(row.name);
    let host = body.host.unwrap_or(row.host);
    let port = body.port.unwrap_or(row.port);
    let autostart = body.autostart.unwrap_or(row.autostart);
    let autostart_delay = body.autostart_delay.unwrap_or(row.autostart_delay);
    let crash_detection = body.crash_detection.unwrap_or(row.crash_detection);
    let stop_command = body.stop_command.unwrap_or(row.stop_command);
    let shutdown_timeout = body.shutdown_timeout.unwrap_or(row.shutdown_timeout);
    let ignored_exits = body.ignored_exits.unwrap_or(row.ignored_exits);
    let count_players = body.count_players.unwrap_or(row.count_players);
    let public_status = body.public_status.unwrap_or(row.public_status);
    let log_path = body.log_path.unwrap_or(row.log_path);
    let executable = body.executable.unwrap_or(row.executable);
    let command = body.command.unwrap_or(row.command);
    let java_binary = body.java_binary.or(row.java_binary);
    let min_memory_mb = body.min_memory_mb.unwrap_or(row.min_memory_mb);
    let max_memory_mb = body.max_memory_mb.unwrap_or(row.max_memory_mb);
    let java_flags = body.java_flags.unwrap_or(row.java_flags);

    sqlx::query(
        "UPDATE servers SET name = ?, host = ?, port = ?, autostart = ?, autostart_delay = ?,
            crash_detection = ?, stop_command = ?, shutdown_timeout = ?, ignored_exits = ?,
            count_players = ?, public_status = ?, log_path = ?, executable = ?, command = ?,
            java_binary = ?, min_memory_mb = ?, max_memory_mb = ?, java_flags = ?, updated_at = ?
         WHERE id = ?",
    )
    .bind(&name)
    .bind(&host)
    .bind(port)
    .bind(autostart)
    .bind(autostart_delay)
    .bind(crash_detection)
    .bind(&stop_command)
    .bind(shutdown_timeout)
    .bind(&ignored_exits)
    .bind(count_players)
    .bind(public_status)
    .bind(&log_path)
    .bind(&executable)
    .bind(&command)
    .bind(&java_binary)
    .bind(min_memory_mb)
    .bind(max_memory_mb)
    .bind(&java_flags)
    .bind(Utc::now().to_rfc3339())
    .bind(id.to_string())
    .execute(&state.db)
    .await?;

    crate::api::audit(
        &state,
        Some(&identity.user),
        Some(id),
        "changed server settings",
        Some(&name),
    )
    .await;

    let row = load(&state, id).await?;
    let granted = identity
        .server_permissions(&state.db, id)
        .await
        .map_err(ApiError::Internal)?;
    let names = granted.iter().map(|p| p.as_str()).collect();
    Ok(OkJson(server_json(&state, &row, names).await))
}

#[derive(Deserialize)]
struct Remove {
    #[serde(default)]
    delete_files: bool,
}

async fn remove(
    identity: Identity,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<Remove>,
) -> ApiResult<impl IntoResponse> {
    identity
        .require_server(&state.db, id, ServerPerm::Config)
        .await?;
    if state.supervisor.state(id).await.is_live() {
        return Err(ApiError::conflict("Stop the server before deleting it."));
    }
    let row = load(&state, id).await?;

    sqlx::query("DELETE FROM servers WHERE id = ?")
        .bind(id.to_string())
        .execute(&state.db)
        .await?;

    if query.delete_files {
        let directory = std::path::PathBuf::from(&row.directory);
        if directory.starts_with(&state.config.paths.servers)
            && let Err(error) = tokio::fs::remove_dir_all(&directory).await
        {
            tracing::warn!(%error, path = %directory.display(), "could not remove server files");
        }
    }

    state.statuses.forget(id).await;
    crate::api::audit(
        &state,
        Some(&identity.user),
        Some(id),
        "deleted a server",
        Some(&row.name),
    )
    .await;
    state.events.publish(
        crate::events::Topic::Servers,
        "deleted",
        json!({ "server_id": id }),
    );
    Ok(Done)
}

#[derive(Deserialize)]
struct Action {
    action: String,
}

async fn action(
    identity: Identity,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<Action>,
) -> ApiResult<impl IntoResponse> {
    identity
        .require_server(&state.db, id, ServerPerm::Commands)
        .await?;
    let row = load(&state, id).await?;

    let supervisor = state.supervisor.clone();
    let result = match body.action.as_str() {
        "start" => supervisor.start_by_id(id).await,
        "stop" => supervisor.stop(id).await,
        "restart" => supervisor.restart(id).await,
        "kill" => supervisor.kill(id).await,
        other => {
            return Err(ApiError::validation(format!("Unknown action '{other}'.")));
        }
    };

    result.map_err(|error| ApiError::conflict(error.to_string()))?;
    crate::api::audit(
        &state,
        Some(&identity.user),
        Some(id),
        &format!("sent {} to a server", body.action),
        Some(&row.name),
    )
    .await;
    Ok(Done)
}

#[derive(Deserialize)]
struct CommandBody {
    command: String,
}

async fn command(
    identity: Identity,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<CommandBody>,
) -> ApiResult<impl IntoResponse> {
    identity
        .require_server(&state.db, id, ServerPerm::Commands)
        .await?;
    if body.command.trim().is_empty() {
        return Err(ApiError::field("command", "Type a command first."));
    }
    let outcome = state
        .supervisor
        .run_command(id, body.command.trim())
        .await
        .map_err(|error| ApiError::conflict(error.to_string()))?;
    crate::api::audit(
        &state,
        Some(&identity.user),
        Some(id),
        "ran a console command",
        Some(body.command.trim()),
    )
    .await;
    Ok(OkJson(
        json!({ "via": outcome.via, "output": outcome.output }),
    ))
}

/// Only Java servers speak RCON; Bedrock has no such listener.
fn speaks_rcon(row: &ServerRow) -> bool {
    row.kind == "minecraft_java"
}

async fn rcon_status(
    identity: Identity,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<impl IntoResponse> {
    identity
        .require_server(&state.db, id, ServerPerm::Commands)
        .await?;
    let row = load(&state, id).await?;
    let directory = PathBuf::from(&row.directory);

    let settings = match speaks_rcon(&row) {
        true => crate::rcon::settings(&directory)
            .await
            .map_err(ApiError::Internal)?,
        false => None,
    };
    let configured = settings
        .as_ref()
        .is_some_and(|settings| settings.enabled && settings.has_password);

    // Proving it works needs an actual connection, which only exists while the
    // server is up.
    let mut reachable = false;
    if configured
        && state.supervisor.state(id).await.is_live()
        && let Ok(Some(endpoint)) = crate::rcon::endpoint(&directory).await
    {
        reachable = crate::rcon::check(&endpoint).await.is_ok();
    }

    Ok(OkJson(json!({
        "supported": speaks_rcon(&row),
        "enabled": configured,
        "port": settings.as_ref().and_then(|settings| settings.port),
        "reachable": reachable,
    })))
}

#[derive(Deserialize)]
struct RconEnable {
    /// Replace a password that is already in the file.
    #[serde(default)]
    regenerate_password: bool,
}

async fn rcon_enable(
    identity: Identity,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<RconEnable>,
) -> ApiResult<impl IntoResponse> {
    identity
        .require_server(&state.db, id, ServerPerm::Config)
        .await?;
    let row = load(&state, id).await?;
    if !speaks_rcon(&row) {
        return Err(ApiError::validation(
            "Only Java servers speak RCON. Bedrock takes commands on the console only.",
        ));
    }

    let directory = PathBuf::from(&row.directory);
    let Some(mut file) = Properties::load_if_present(properties::path_in(&directory))
        .await
        .map_err(ApiError::Internal)?
    else {
        return Err(ApiError::conflict(
            "Start the server once so it writes server.properties, then turn RCON on.",
        ));
    };

    let port = match file
        .number("rcon.port")
        .filter(|port| *port > 0 && *port < 65536)
    {
        Some(port) => port,
        None => {
            let taken = reserved_ports(&state, &row).await;
            crate::rcon::free_port(&taken)
                .await
                .map(i64::from)
                .ok_or_else(|| ApiError::conflict("No free port for the RCON listener."))?
        }
    };

    let password = match file.get("rcon.password") {
        Some(existing) if !existing.is_empty() && !body.regenerate_password => existing,
        _ => crate::rcon::generate_password(),
    };

    file.set("enable-rcon", "true");
    file.set("rcon.port", &port.to_string());
    file.set("rcon.password", &password);
    file.save().await.map_err(ApiError::Internal)?;

    crate::api::audit(
        &state,
        Some(&identity.user),
        Some(id),
        "turned on RCON",
        Some(&row.name),
    )
    .await;

    Ok(OkJson(json!({
        "port": port,
        "restart_required": state.supervisor.state(id).await.is_live(),
    })))
}

/// Ports the panel should not hand to a new RCON listener.
async fn reserved_ports(state: &AppState, row: &ServerRow) -> Vec<i64> {
    let mut taken = vec![state.config.http.port as i64, row.port];
    let others: Vec<(String, i64)> =
        sqlx::query_as("SELECT directory, port FROM servers WHERE id != ?")
            .bind(&row.id)
            .fetch_all(&state.db)
            .await
            .unwrap_or_default();

    for (directory, port) in others {
        taken.push(port);
        if let Ok(Some(settings)) = crate::rcon::settings(&PathBuf::from(directory)).await {
            taken.extend(settings.port);
        }
    }
    taken
}

#[derive(Deserialize)]
struct ConsoleQuery {
    after: Option<u64>,
}

async fn console(
    identity: Identity,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<ConsoleQuery>,
) -> ApiResult<impl IntoResponse> {
    identity
        .require_server(&state.db, id, ServerPerm::Console)
        .await?;
    let lines = state.supervisor.console(id, query.after).await;
    Ok(OkJson(json!({ "lines": lines })))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(id: &str, seconds: i64, cpu: f64) -> StatRow {
        StatRow {
            server_id: id.to_string(),
            at: (since() + chrono::Duration::seconds(seconds)).to_rfc3339(),
            cpu_percent: cpu,
            memory_percent: cpu * 2.0,
        }
    }

    fn since() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
            .expect("fixed")
            .with_timezone(&Utc)
    }

    /// Ten minutes into five buckets: two minutes each.
    fn line(rows: Vec<StatRow>) -> Vec<Value> {
        bucket(rows.into_iter(), since(), 10, 5)
            .remove("a")
            .expect("the one server")
    }

    #[test]
    fn samples_land_in_the_bucket_their_time_falls_in() {
        let found = line(vec![
            sample("a", 0, 10.0),
            sample("a", 60, 20.0),
            sample("a", 480, 50.0),
        ]);
        assert_eq!(found[0]["cpu"], 15.0, "the first two minutes averaged");
        assert_eq!(found[4]["cpu"], 50.0, "eight minutes in is the last bucket");
    }

    #[test]
    fn a_stretch_with_no_samples_is_a_gap_not_a_floor() {
        let found = line(vec![sample("a", 0, 10.0)]);
        assert_eq!(found[0]["cpu"], 10.0);
        for (at, value) in found.iter().enumerate().skip(1) {
            assert!(value.is_null(), "bucket {at} had nothing in it");
        }
    }

    #[test]
    fn every_line_is_the_length_that_was_asked_for() {
        assert_eq!(line(vec![sample("a", 30, 1.0)]).len(), 5);
    }

    #[test]
    fn a_sample_on_the_far_edge_does_not_run_off_the_end() {
        // Exactly the window, which divides into a bucket that is not there.
        let found = line(vec![sample("a", 600, 99.0)]);
        assert_eq!(found[4]["cpu"], 99.0);
    }

    #[test]
    fn a_sample_from_before_the_window_is_left_out() {
        assert!(bucket(vec![sample("a", -60, 5.0)].into_iter(), since(), 10, 5).is_empty());
    }

    #[test]
    fn each_server_gets_its_own_line() {
        let found = bucket(
            vec![sample("a", 0, 10.0), sample("b", 0, 20.0)].into_iter(),
            since(),
            10,
            5,
        );
        assert_eq!(found["a"][0]["cpu"], 10.0);
        assert_eq!(found["b"][0]["cpu"], 20.0);
    }
}
