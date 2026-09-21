use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::{
    Json, Router,
    routing::{get, patch},
};
use chrono::Utc;
use serde::Deserialize;
use serde_json::json;
use sqlx::FromRow;
use uuid::Uuid;

use crate::auth::Identity;
use crate::error::{ApiError, ApiResult, Created, Done, Ok as OkJson};
use crate::perms::Server as ServerPerm;
use crate::state::AppState;

/// Merged into the servers router, where the id in the path comes from.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/{id}/macros", get(list).post(create))
        .route("/{id}/macros/{macro_id}", patch(update).delete(remove))
}

/// Anyone who may run a command may use a macro; saving one is a config change,
/// since it is a button everybody with the server will see.
async fn may_read(identity: &Identity, state: &AppState, id: Uuid) -> Result<(), ApiError> {
    identity
        .require_server(&state.db, id, ServerPerm::Commands)
        .await?;
    Ok(())
}

async fn may_write(identity: &Identity, state: &AppState, id: Uuid) -> Result<(), ApiError> {
    identity
        .require_server(&state.db, id, ServerPerm::Config)
        .await?;
    Ok(())
}

#[derive(FromRow)]
struct Row {
    id: String,
    label: String,
    command: String,
    position: i64,
}

async fn list(
    identity: Identity,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<impl IntoResponse> {
    may_read(&identity, &state, id).await?;
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT id, label, command, position FROM server_macros
         WHERE server_id = ? ORDER BY position, label COLLATE NOCASE",
    )
    .bind(id.to_string())
    .fetch_all(&state.db)
    .await?;

    Ok(OkJson(
        rows.into_iter()
            .map(|row| {
                json!({
                    "id": row.id,
                    "label": row.label,
                    "command": row.command,
                    "position": row.position,
                })
            })
            .collect::<Vec<_>>(),
    ))
}

#[derive(Deserialize)]
struct Write {
    label: Option<String>,
    command: Option<String>,
    position: Option<i64>,
}

pub(crate) fn checked(label: &str, command: &str) -> Result<(String, String), ApiError> {
    let label = label.trim();
    let command = command.trim();
    if label.is_empty() {
        return Err(ApiError::field("label", "Give the button a name."));
    }
    if label.chars().count() > 48 {
        return Err(ApiError::field(
            "label",
            "That name is too long for a button.",
        ));
    }
    if command.is_empty() {
        return Err(ApiError::field("command", "Say what it should run."));
    }
    if command.contains(['\n', '\r']) {
        return Err(ApiError::field("command", "One command per button."));
    }
    Ok((label.to_string(), command.to_string()))
}

async fn create(
    identity: Identity,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<Write>,
) -> ApiResult<impl IntoResponse> {
    may_write(&identity, &state, id).await?;
    let (label, command) = checked(
        body.label.as_deref().unwrap_or_default(),
        body.command.as_deref().unwrap_or_default(),
    )?;

    // New buttons go on the end unless somebody says otherwise.
    let position = match body.position {
        Some(position) => position,
        None => {
            let (last,): (Option<i64>,) =
                sqlx::query_as("SELECT MAX(position) FROM server_macros WHERE server_id = ?")
                    .bind(id.to_string())
                    .fetch_one(&state.db)
                    .await?;
            last.unwrap_or(-1) + 1
        }
    };

    let macro_id = Uuid::new_v4();
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO server_macros (id, server_id, label, command, position, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(macro_id.to_string())
    .bind(id.to_string())
    .bind(&label)
    .bind(&command)
    .bind(position)
    .bind(&now)
    .bind(&now)
    .execute(&state.db)
    .await?;

    super::audit(
        &state,
        Some(&identity.user),
        Some(id),
        "saved a command button",
        Some(&label),
    )
    .await;
    Ok(Created(json!({ "id": macro_id })))
}

async fn update(
    identity: Identity,
    State(state): State<AppState>,
    Path((id, macro_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<Write>,
) -> ApiResult<impl IntoResponse> {
    may_write(&identity, &state, id).await?;
    let row: Row = sqlx::query_as(
        "SELECT id, label, command, position FROM server_macros WHERE id = ? AND server_id = ?",
    )
    .bind(macro_id.to_string())
    .bind(id.to_string())
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| ApiError::not_found("Command button"))?;

    let (label, command) = checked(
        body.label.as_deref().unwrap_or(&row.label),
        body.command.as_deref().unwrap_or(&row.command),
    )?;

    sqlx::query(
        "UPDATE server_macros SET label = ?, command = ?, position = ?, updated_at = ?
         WHERE id = ?",
    )
    .bind(&label)
    .bind(&command)
    .bind(body.position.unwrap_or(row.position))
    .bind(Utc::now().to_rfc3339())
    .bind(macro_id.to_string())
    .execute(&state.db)
    .await?;

    Ok(Done)
}

async fn remove(
    identity: Identity,
    State(state): State<AppState>,
    Path((id, macro_id)): Path<(Uuid, Uuid)>,
) -> ApiResult<impl IntoResponse> {
    may_write(&identity, &state, id).await?;
    let done = sqlx::query("DELETE FROM server_macros WHERE id = ? AND server_id = ?")
        .bind(macro_id.to_string())
        .bind(id.to_string())
        .execute(&state.db)
        .await?;
    if done.rows_affected() == 0 {
        return Err(ApiError::not_found("Command button"));
    }
    Ok(Done)
}
