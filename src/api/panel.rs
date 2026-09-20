use axum::extract::{Query, State};
use axum::response::IntoResponse;
use axum::{Router, routing::get};
use serde::Deserialize;
use serde_json::json;
use sysinfo::Disks;

use crate::auth::Identity;
use crate::error::{ApiResult, Ok as OkJson};
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/stats", get(stats))
        .route("/audit", get(audit))
        .route("/java", get(java))
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

    let disks = Disks::new_with_refreshed_list();
    let disks: Vec<_> = disks
        .list()
        .iter()
        .map(|disk| {
            let total = disk.total_space();
            let free = disk.available_space();
            json!({
                "mount": disk.mount_point().to_string_lossy(),
                "filesystem": disk.file_system().to_string_lossy(),
                "total_bytes": total,
                "used_bytes": total.saturating_sub(free),
                "free_bytes": free,
                "used_percent": if total > 0 { (total - free) as f64 / total as f64 * 100.0 } else { 0.0 },
            })
        })
        .collect();

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
