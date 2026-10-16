use std::path::PathBuf;

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::{Json, Router, routing::get};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::auth::Identity;
use crate::enshrouded::{self, Group};
use crate::error::{ApiError, ApiResult, Done, Ok as OkJson};
use crate::perms::Server as ServerPerm;
use crate::state::AppState;

use super::servers::ServerRow;

/// Merged into the servers router, where the id in the path comes from.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/{id}/enshrouded/roles", get(roles).put(save_roles))
        .route("/{id}/enshrouded/bans", get(bans))
        .route(
            "/{id}/enshrouded/bans/{index}",
            axum::routing::delete(unban),
        )
}

async fn load_enshrouded(state: &AppState, id: Uuid) -> Result<ServerRow, ApiError> {
    let row = super::servers::load(state, id).await?;
    if row.kind != enshrouded::KIND {
        return Err(ApiError::validation("Only Enshrouded servers answer here."));
    }
    Ok(row)
}

async fn read_config(row: &ServerRow) -> Result<Value, ApiError> {
    enshrouded::load(&PathBuf::from(&row.directory))
        .await
        .map_err(ApiError::Internal)?
        .ok_or_else(|| ApiError::not_found("enshrouded_server.json"))
}

/// The game rewrites its file while it runs, so an edit then would be lost.
async fn require_stopped(state: &AppState, id: Uuid) -> Result<(), ApiError> {
    match state.supervisor.state(id).await.is_live() {
        true => Err(ApiError::conflict(
            "Stop the server first: Enshrouded rewrites this file while it runs.",
        )),
        false => Ok(()),
    }
}

async fn roles(
    identity: Identity,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<impl IntoResponse> {
    identity
        .require_server(&state.db, id, ServerPerm::Config)
        .await?;
    let row = load_enshrouded(&state, id).await?;
    let config = read_config(&row).await?;
    Ok(OkJson(json!({
        "running": state.supervisor.state(id).await.is_live(),
        "slots": enshrouded::slots(&config),
        "roles": enshrouded::groups(&config),
    })))
}

#[derive(Deserialize)]
struct Roles {
    roles: Vec<Group>,
}

async fn save_roles(
    identity: Identity,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<Roles>,
) -> ApiResult<impl IntoResponse> {
    identity
        .require_server(&state.db, id, ServerPerm::Config)
        .await?;
    let row = load_enshrouded(&state, id).await?;
    require_stopped(&state, id).await?;
    let mut config = read_config(&row).await?;

    let roles: Vec<Group> = body
        .roles
        .into_iter()
        .map(|mut role| {
            role.name = role.name.trim().to_string();
            role
        })
        .collect();
    enshrouded::check_groups(&roles, enshrouded::slots(&config)).map_err(ApiError::validation)?;
    enshrouded::set_groups(&mut config, &roles);
    enshrouded::save(&PathBuf::from(&row.directory), &config)
        .await
        .map_err(ApiError::Internal)?;

    let names: Vec<&str> = roles.iter().map(|role| role.name.as_str()).collect();
    super::audit(
        &state,
        Some(&identity.user),
        Some(id),
        "changed the Enshrouded roles",
        Some(&names.join(", ")),
    )
    .await;
    Ok(OkJson(json!({ "roles": roles })))
}

async fn bans(
    identity: Identity,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<impl IntoResponse> {
    identity
        .require_server(&state.db, id, ServerPerm::Players)
        .await?;
    let row = load_enshrouded(&state, id).await?;
    let config = read_config(&row).await?;
    let bans: Vec<Value> = enshrouded::bans(&config)
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let text = |key: &str| entry.get(key).and_then(Value::as_str).map(str::to_string);
            // Written as a number before 0.9 and as `{"value": n}` since.
            let date = entry
                .get("banDate")
                .and_then(|date| date.as_i64().or_else(|| date.get("value")?.as_i64()));
            json!({
                "index": index,
                "name": text("displayName"),
                "character": text("characterName"),
                "banned_at": date,
            })
        })
        .collect();
    Ok(OkJson(json!({
        "running": state.supervisor.state(id).await.is_live(),
        "bans": bans,
    })))
}

async fn unban(
    identity: Identity,
    State(state): State<AppState>,
    Path((id, index)): Path<(Uuid, usize)>,
) -> ApiResult<impl IntoResponse> {
    identity
        .require_server(&state.db, id, ServerPerm::Players)
        .await?;
    let row = load_enshrouded(&state, id).await?;
    require_stopped(&state, id).await?;
    let mut config = read_config(&row).await?;
    let removed =
        enshrouded::remove_ban(&mut config, index).ok_or_else(|| ApiError::not_found("Ban"))?;
    enshrouded::save(&PathBuf::from(&row.directory), &config)
        .await
        .map_err(ApiError::Internal)?;
    let name = removed
        .get("displayName")
        .and_then(Value::as_str)
        .unwrap_or("someone")
        .to_string();
    super::audit(
        &state,
        Some(&identity.user),
        Some(id),
        "lifted an Enshrouded ban",
        Some(&name),
    )
    .await;
    Ok(Done)
}
