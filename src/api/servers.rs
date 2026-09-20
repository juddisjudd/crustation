use std::path::PathBuf;

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::{
    Json, Router,
    routing::{get, post},
};
use chrono::Utc;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::FromRow;
use uuid::Uuid;

use crate::auth::Identity;
use crate::error::{ApiError, ApiResult, Done, Ok as OkJson};
use crate::perms::Server as ServerPerm;
use crate::properties::{self, Properties};
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(list))
        .route("/{id}", get(detail).patch(update).delete(remove))
        .route("/{id}/action", post(action))
        .route("/{id}/command", post(command))
        .route("/{id}/console", get(console))
        .route("/{id}/rcon", get(rcon_status).post(rcon_enable))
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

    match row {
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
    }
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
        if let Some(allowed) = &visible {
            if !allowed.contains(&row.id) {
                continue;
            }
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

    if let Some(name) = &body.name {
        if name.trim().is_empty() {
            return Err(ApiError::field("name", "Give the server a name."));
        }
    }
    if let (Some(min), Some(max)) = (body.min_memory_mb, body.max_memory_mb) {
        if min > max {
            return Err(ApiError::field(
                "min_memory_mb",
                "Minimum memory cannot exceed the maximum.",
            ));
        }
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
        if directory.starts_with(&state.config.paths.servers) {
            if let Err(error) = tokio::fs::remove_dir_all(&directory).await {
                tracing::warn!(%error, path = %directory.display(), "could not remove server files");
            }
        }
    }

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
    if configured && state.supervisor.state(id).await.is_live() {
        if let Ok(Some(endpoint)) = crate::rcon::endpoint(&directory).await {
            reachable = crate::rcon::check(&endpoint).await.is_ok();
        }
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
