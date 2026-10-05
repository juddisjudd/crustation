use axum::extract::{Path, State};
use axum::http::header;
use axum::response::IntoResponse;
use axum::{
    Json, Router,
    routing::{delete, get, post},
};
use serde::Deserialize;
use serde_json::json;
use sqlx::FromRow;
use uuid::Uuid;

use crate::auth::Identity;
use crate::error::{ApiError, ApiResult, Done, Ok as OkJson};
use crate::perms::Server as ServerPerm;
use crate::rust_moderation as moderation;
use crate::state::AppState;

/// Merged into the servers router, where the id in the path comes from.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/{id}/rust/reports", get(reports).put(inbox))
        .route("/{id}/rust/reports/{report_id}", delete(forget))
        .route("/{id}/rust/reports/{report_id}/image", get(image))
        .route("/{id}/rust/shared-bans", get(shared).put(share))
}

/// The panel-wide list itself, which only an administrator edits.
pub fn panel_routes() -> Router<AppState> {
    Router::new()
        .route("/", post(add_ban))
        .route("/{steam_id}", delete(lift_ban))
}

async fn rust_command(state: &AppState, id: Uuid) -> Result<String, ApiError> {
    let row = super::servers::load(state, id).await?;
    if row.kind != crate::rust_game::KIND {
        return Err(ApiError::validation("Only Rust servers answer here."));
    }
    Ok(row.command)
}

#[derive(FromRow)]
struct ReportRow {
    id: String,
    received_at: String,
    reporter_id: String,
    reporter_name: Option<String>,
    kind: i64,
    subject: String,
    message: String,
    target_id: Option<String>,
    target_name: Option<String>,
    has_image: bool,
}

async fn reports(
    identity: Identity,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<impl IntoResponse> {
    identity
        .require_server(&state.db, id, ServerPerm::Players)
        .await?;
    let command = rust_command(&state, id).await?;
    let rows: Vec<ReportRow> = sqlx::query_as(
        "SELECT id, received_at, reporter_id, reporter_name, kind, subject, message, target_id,
                target_name, image IS NOT NULL AS has_image
         FROM rust_reports WHERE server_id = ? ORDER BY received_at DESC LIMIT 200",
    )
    .bind(id.to_string())
    .fetch_all(&state.db)
    .await?;
    let reports: Vec<_> = rows
        .iter()
        .map(|row| {
            json!({
                "id": row.id,
                "received_at": row.received_at,
                "reporter_id": row.reporter_id,
                "reporter_name": row.reporter_name,
                "kind": row.kind,
                "subject": row.subject,
                "message": row.message,
                "target_id": row.target_id,
                "target_name": row.target_name,
                "has_image": row.has_image,
            })
        })
        .collect();
    let enabled = crate::rust_game::convar(&command, moderation::REPORTS_URL).is_some();
    Ok(OkJson(json!({ "enabled": enabled, "reports": reports })))
}

#[derive(Deserialize)]
struct Toggle {
    enabled: bool,
}

async fn inbox(
    identity: Identity,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<Toggle>,
) -> ApiResult<impl IntoResponse> {
    identity
        .require_server(&state.db, id, ServerPerm::Config)
        .await?;
    rust_command(&state, id).await?;
    moderation::use_inbox(&state, id, body.enabled)
        .await
        .map_err(ApiError::Internal)?;
    let what = match body.enabled {
        true => "started collecting F7 reports",
        false => "stopped collecting F7 reports",
    };
    super::audit(&state, Some(&identity.user), Some(id), what, None).await;
    Ok(OkJson(json!({ "enabled": body.enabled })))
}

async fn image(
    identity: Identity,
    State(state): State<AppState>,
    Path((id, report_id)): Path<(Uuid, String)>,
) -> ApiResult<impl IntoResponse> {
    identity
        .require_server(&state.db, id, ServerPerm::Players)
        .await?;
    let image: Option<Vec<u8>> =
        sqlx::query_scalar("SELECT image FROM rust_reports WHERE id = ? AND server_id = ?")
            .bind(&report_id)
            .bind(id.to_string())
            .fetch_optional(&state.db)
            .await?
            .flatten();
    let image = image.ok_or_else(|| ApiError::not_found("Screenshot"))?;
    Ok(([(header::CONTENT_TYPE, "image/jpeg")], image))
}

async fn forget(
    identity: Identity,
    State(state): State<AppState>,
    Path((id, report_id)): Path<(Uuid, String)>,
) -> ApiResult<impl IntoResponse> {
    identity
        .require_server(&state.db, id, ServerPerm::Players)
        .await?;
    let gone = sqlx::query("DELETE FROM rust_reports WHERE id = ? AND server_id = ?")
        .bind(&report_id)
        .bind(id.to_string())
        .execute(&state.db)
        .await?
        .rows_affected();
    if gone == 0 {
        return Err(ApiError::not_found("Report"));
    }
    Ok(Done)
}

#[derive(FromRow, serde::Serialize)]
struct SharedBan {
    steam_id: String,
    name: String,
    reason: String,
    expires_at: Option<i64>,
    mute: bool,
    created_at: String,
    created_by: Option<String>,
}

async fn shared(
    identity: Identity,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<impl IntoResponse> {
    identity
        .require_server(&state.db, id, ServerPerm::Players)
        .await?;
    let command = rust_command(&state, id).await?;
    let bans: Vec<SharedBan> =
        sqlx::query_as("SELECT * FROM rust_shared_bans ORDER BY created_at DESC")
            .fetch_all(&state.db)
            .await?;
    Ok(OkJson(json!({
        "enabled": crate::rust_game::convar(&command, moderation::BANS_URL).is_some(),
        "can_edit": identity.is_admin(),
        "bans": bans,
    })))
}

async fn share(
    identity: Identity,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<Toggle>,
) -> ApiResult<impl IntoResponse> {
    identity
        .require_server(&state.db, id, ServerPerm::Config)
        .await?;
    rust_command(&state, id).await?;
    moderation::use_shared_bans(&state, id, body.enabled)
        .await
        .map_err(ApiError::Internal)?;
    let what = match body.enabled {
        true => "joined the shared ban list",
        false => "left the shared ban list",
    };
    super::audit(&state, Some(&identity.user), Some(id), what, None).await;
    Ok(OkJson(json!({ "enabled": body.enabled })))
}

#[derive(Deserialize)]
struct NewBan {
    steam_id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    reason: String,
    duration: Option<String>,
    #[serde(default)]
    mute: bool,
}

fn admin(identity: &Identity) -> Result<(), ApiError> {
    if !identity.is_admin() {
        return Err(ApiError::forbidden(
            "Only an administrator edits the ban list every server shares.",
        ));
    }
    Ok(())
}

fn plain(field: &'static str, value: &str, max: usize) -> Result<String, ApiError> {
    let value = value.trim();
    if value.chars().any(|c| c.is_control() || c == '"') || value.chars().count() > max {
        return Err(ApiError::field(
            field,
            format!("Keep it to one line of up to {max} characters, without double quotes."),
        ));
    }
    Ok(value.to_string())
}

/// Adds a ban everywhere and applies it at once, since Rust only asks when somebody joins.
async fn add_ban(
    identity: Identity,
    State(state): State<AppState>,
    Json(body): Json<NewBan>,
) -> ApiResult<impl IntoResponse> {
    admin(&identity)?;
    let steam_id = body.steam_id.trim().to_string();
    if !crate::rust_players::is_steam_id(&steam_id) {
        return Err(ApiError::field(
            "steam_id",
            "That is not a Steam id: 17 digits starting 7656.",
        ));
    }
    let name = plain("name", &body.name, 64)?;
    let reason = plain("reason", &body.reason, 128)?;
    let expires = match body
        .duration
        .as_deref()
        .map(str::trim)
        .filter(|d| !d.is_empty())
    {
        Some(length) => Some(
            crate::rust_players::duration_seconds(length)
                .map(|seconds| chrono::Utc::now().timestamp() + seconds)
                .ok_or_else(|| ApiError::field("duration", "Use hours or days, like 12h or 7d."))?,
        ),
        None => None,
    };
    sqlx::query(
        "INSERT INTO rust_shared_bans (steam_id, name, reason, expires_at, mute, created_at, created_by)
         VALUES (?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(steam_id) DO UPDATE SET name = excluded.name, reason = excluded.reason,
             expires_at = excluded.expires_at, mute = excluded.mute",
    )
    .bind(&steam_id)
    .bind(&name)
    .bind(&reason)
    .bind(expires)
    .bind(body.mute)
    .bind(chrono::Utc::now().to_rfc3339())
    .bind(&identity.user.username)
    .execute(&state.db)
    .await?;

    let servers: Vec<(String, String)> =
        sqlx::query_as("SELECT id, command FROM servers WHERE kind = ?")
            .bind(crate::rust_game::KIND)
            .fetch_all(&state.db)
            .await?;
    let command = match body.mute {
        true => format!("mute {steam_id}"),
        false => format!("kick {steam_id} \"{reason}\""),
    };
    for (id, server_command) in servers {
        let Ok(id) = Uuid::parse_str(&id) else {
            continue;
        };
        let shares = crate::rust_game::convar(&server_command, moderation::BANS_URL).is_some();
        if shares && state.supervisor.state(id).await.is_live() {
            state.supervisor.ask(id, &command).await.ok();
        }
    }

    super::audit(
        &state,
        Some(&identity.user),
        None,
        "banned on every Rust server",
        Some(&steam_id),
    )
    .await;
    Ok(OkJson(json!({ "steam_id": steam_id })))
}

async fn lift_ban(
    identity: Identity,
    State(state): State<AppState>,
    Path(steam_id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    admin(&identity)?;
    let gone = sqlx::query("DELETE FROM rust_shared_bans WHERE steam_id = ?")
        .bind(&steam_id)
        .execute(&state.db)
        .await?
        .rows_affected();
    if gone == 0 {
        return Err(ApiError::not_found("Ban"));
    }
    super::audit(
        &state,
        Some(&identity.user),
        None,
        "lifted a shared ban",
        Some(&steam_id),
    )
    .await;
    Ok(Done)
}
