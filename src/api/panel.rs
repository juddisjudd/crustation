use axum::extract::{Query, State};
use axum::response::IntoResponse;
use axum::{Router, routing::get};
use serde::Deserialize;
use serde_json::json;
use sysinfo::Disks;

use crate::auth::Identity;
use crate::error::{ApiError, ApiResult, Ok as OkJson};
use crate::perms::Global;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/stats", get(stats))
        .route("/audit", get(audit))
        .route("/java", get(java))
        .route("/mcp", get(mcp).patch(set_mcp))
}

/// What the MCP screen shows: whether the endpoint answers, where a client
/// should point, and what a key would be able to do once it got there.
async fn mcp(identity: Identity, State(state): State<AppState>) -> ApiResult<impl IntoResponse> {
    identity.require_global(Global::Admin)?;
    Ok(OkJson(json!({
        "enabled": state.mcp_on(),
        "config_default": state.config.panel.mcp_enabled,
        // Empty when nothing is configured, and the interface falls back to the
        // address the browser is already using, which is right often enough.
        "public_url": state.config.http.public_url,
        "tools": crate::mcp::described(),
        "resources": [
            "crustation://servers",
            "crustation://servers/{id}/details",
            "crustation://servers/{id}/console",
            "crustation://servers/{id}/properties",
        ],
    })))
}

#[derive(Deserialize)]
struct SetMcp {
    enabled: bool,
}

async fn set_mcp(
    identity: Identity,
    State(state): State<AppState>,
    axum::Json(body): axum::Json<SetMcp>,
) -> ApiResult<impl IntoResponse> {
    identity.require_global(Global::Admin)?;
    state
        .set_mcp(body.enabled)
        .await
        .map_err(ApiError::Internal)?;

    super::audit(
        &state,
        Some(&identity.user),
        None,
        match body.enabled {
            true => "turned the MCP endpoint on",
            false => "turned the MCP endpoint off",
        },
        None,
    )
    .await;
    Ok(OkJson(json!({ "enabled": state.mcp_on() })))
}

async fn stats(_identity: Identity, State(state): State<AppState>) -> ApiResult<impl IntoResponse> {
    let mut system = sysinfo::System::new();
    system.refresh_memory();
    system.refresh_cpu_usage();
    // One sample is meaningless for CPU; take a second after a short pause.
    tokio::time::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL).await;
    system.refresh_cpu_usage();

    let cpu_percent = system.global_cpu_usage();
    let memory_total = system.total_memory();
    let memory_used = memory_total.saturating_sub(system.available_memory());

    let disks = storage(&state.config);

    let running = state.supervisor.running_ids().await.len();
    let (total,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM servers")
        .fetch_one(&state.db)
        .await?;

    Ok(OkJson(json!({
        "cpu": { "percent": cpu_percent, "cores": num_cpus() },
        "memory": {
            "total_bytes": memory_total,
            "used_bytes": memory_used,
            "used_percent": if memory_total > 0 { memory_used as f64 / memory_total as f64 * 100.0 } else { 0.0 },
        },
        "disks": disks,
        "servers": { "total": total, "running": running, "stopped": (total as usize).saturating_sub(running) },
        "uptime_seconds": (chrono::Utc::now() - state.started_at).num_seconds(),
    })))
}

/// The disks worth showing: the ones the panel's own folders sit on.
///
/// Listing every mount the host reports is useless in a container. Docker binds
/// `/etc/hosts`, `/etc/hostname` and `/etc/resolv.conf` in as single files, so
/// they arrive looking like three more disks all reporting the container's root
/// filesystem, and on Unraid the config and servers shares are usually the same
/// array, listed twice. What an operator wants to know is whether the worlds and
/// the backups have room, so that is what this answers: one row per real
/// filesystem, saying which of the panel's folders live on it.
fn storage(config: &crate::config::Config) -> Vec<serde_json::Value> {
    let disks = Disks::new_with_refreshed_list();
    let wanted = [
        ("servers", &config.paths.servers),
        ("backups", &config.paths.backups),
        ("config", &config.paths.config),
    ];

    // Mount point -> the folders that live on it, in the order asked for.
    let mut grouped: Vec<(std::path::PathBuf, Vec<&str>)> = Vec::new();
    for (label, path) in wanted {
        let Some(disk) = holding(&disks, path) else {
            continue;
        };
        let mount = disk.mount_point().to_path_buf();
        match grouped.iter_mut().find(|(at, _)| *at == mount) {
            Some((_, labels)) => labels.push(label),
            None => grouped.push((mount, vec![label])),
        }
    }

    grouped
        .into_iter()
        .filter_map(|(mount, keeps)| {
            let disk = disks.list().iter().find(|one| one.mount_point() == mount)?;
            let total = disk.total_space();
            let free = disk.available_space();
            Some(json!({
                "mount": mount.to_string_lossy(),
                "keeps": keeps,
                "filesystem": disk.file_system().to_string_lossy(),
                "total_bytes": total,
                "used_bytes": total.saturating_sub(free),
                "free_bytes": free,
                "used_percent": if total > 0 {
                    total.saturating_sub(free) as f64 / total as f64 * 100.0
                } else {
                    0.0
                },
            }))
        })
        .collect()
}

/// The disk a folder sits on: the one whose mount point is the longest prefix of
/// it, since `/` matches everything and the real answer is more specific.
///
/// The path is made absolute rather than canonical, because on Windows
/// `canonicalize` hands back a `\\?\C:\…` verbatim path, and that prefix means
/// it no longer starts with the `C:\` a mount point is reported as.
fn holding<'a>(disks: &'a Disks, path: &std::path::Path) -> Option<&'a sysinfo::Disk> {
    let full = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    disks
        .list()
        .iter()
        .filter(|disk| full.starts_with(disk.mount_point()))
        .max_by_key(|disk| disk.mount_point().as_os_str().len())
}

fn num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(|value| value.get())
        .unwrap_or(1)
}

#[derive(sqlx::FromRow)]
struct AuditRow {
    id: i64,
    at: String,
    username: Option<String>,
    server_id: Option<String>,
    action: String,
    detail: Option<String>,
    address: Option<String>,
}

#[derive(Deserialize)]
struct AuditQuery {
    limit: Option<i64>,
    before: Option<i64>,
}

async fn audit(
    identity: Identity,
    State(state): State<AppState>,
    Query(query): Query<AuditQuery>,
) -> ApiResult<impl IntoResponse> {
    identity.require_global(crate::perms::Global::Admin)?;
    let limit = query.limit.unwrap_or(200).clamp(1, 1000);
    let before = query.before.unwrap_or(i64::MAX);

    let rows: Vec<AuditRow> = sqlx::query_as(
        "SELECT id, at, username, server_id, action, detail, address
             FROM audit_log WHERE id < ? ORDER BY id DESC LIMIT ?",
    )
    .bind(before)
    .bind(limit)
    .fetch_all(&state.db)
    .await?;

    let entries: Vec<_> = rows
        .into_iter()
        .map(|row| {
            json!({
                "id": row.id,
                "at": row.at,
                "username": row.username,
                "server_id": row.server_id,
                "action": row.action,
                "detail": row.detail,
                "address": row.address,
            })
        })
        .collect();

    Ok(OkJson(json!({ "entries": entries })))
}

/// Java runtimes we can find, for the server settings page.
async fn java(_identity: Identity) -> ApiResult<impl IntoResponse> {
    Ok(OkJson(json!({ "runtimes": crate::java::discover().await })))
}
