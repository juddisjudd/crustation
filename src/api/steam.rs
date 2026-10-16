use std::path::PathBuf;

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::{Json, Router, routing::get};
use serde::Deserialize;
use serde_json::json;
use sqlx::FromRow;
use uuid::Uuid;

use crate::auth::Identity;
use crate::error::{ApiError, ApiResult, Ok as OkJson};
use crate::perms::Server as ServerPerm;
use crate::state::AppState;

use super::servers::ServerRow;

/// Merged into the servers router, where the id in the path comes from.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/{id}/steam/build", get(build).post(update))
        .route("/{id}/roster", get(roster))
}

async fn load_steam(state: &AppState, id: Uuid) -> Result<ServerRow, ApiError> {
    let row = super::servers::load(state, id).await?;
    if !crate::steam_games::is_steam(&row.kind) {
        return Err(ApiError::validation(
            "Only a game installed through Steam answers here.",
        ));
    }
    Ok(row)
}

/// The build installed against the newest on its branch, asking Steam if the answer is stale.
async fn build(
    identity: Identity,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<impl IntoResponse> {
    identity
        .require_server(&state.db, id, ServerPerm::Config)
        .await?;
    let row = load_steam(&state, id).await?;
    let directory = PathBuf::from(&row.directory);
    let branch = row.provider_version.clone().unwrap_or_default();
    let build = match crate::steam_updates::check(&state, &row.kind, &directory, &branch).await {
        Ok(build) => build,
        Err(error) => {
            tracing::debug!(%error, "could not ask Steam for the current build");
            crate::steam_updates::known(&row.kind, &directory, &branch)
        }
    };
    Ok(OkJson(json!(build)))
}

#[derive(Deserialize)]
struct UpdateRequest {
    /// How long players get to finish up, in seconds.
    #[serde(default)]
    warn: u64,
}

async fn update(
    identity: Identity,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateRequest>,
) -> ApiResult<impl IntoResponse> {
    identity
        .require_server(&state.db, id, ServerPerm::Config)
        .await?;
    let row = load_steam(&state, id).await?;
    let runtime = state.supervisor.runtime(id).await;
    if runtime.installing || runtime.updating {
        return Err(ApiError::conflict(
            "This server is already being installed or updated.",
        ));
    }
    let warn = match crate::steam_games::takes_commands(&row.kind) {
        true => body.warn.min(30 * 60),
        false => 0,
    };
    crate::steam_updates::spawn_update(
        state.clone(),
        id,
        row.kind.clone(),
        PathBuf::from(&row.directory),
        row.provider_version.clone().unwrap_or_default(),
        warn,
    );
    super::audit(
        &state,
        Some(&identity.user),
        Some(id),
        "updated the server from Steam",
        None,
    )
    .await;
    Ok(OkJson(json!({ "started": true, "warn": warn })))
}

#[derive(FromRow)]
struct Seen {
    name: String,
    uuid: Option<String>,
    first_seen: String,
    last_seen: String,
    online: bool,
}

/// Who is on and who has been, as the console announced them; for the games with no list to ask.
async fn roster(
    identity: Identity,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<impl IntoResponse> {
    identity
        .require_server(&state.db, id, ServerPerm::Players)
        .await?;
    let row = super::servers::load(&state, id).await?;
    if !crate::stats::counts_from_console(&row.kind) {
        return Err(ApiError::validation(
            "This kind of server has its own players page.",
        ));
    }
    let seen: Vec<Seen> = sqlx::query_as(
        "SELECT name, uuid, first_seen, last_seen, online FROM server_players
         WHERE server_id = ? ORDER BY online DESC, last_seen DESC",
    )
    .bind(id.to_string())
    .fetch_all(&state.db)
    .await?;
    let running = state.supervisor.state(id).await.is_live();
    let live = state.statuses.get(id).await;
    let online = seen.iter().filter(|player| player.online).count() as i64;
    let known: Vec<_> = seen
        .iter()
        .map(|player| {
            json!({
                "name": player.name,
                "id": player.uuid,
                "first_seen": player.first_seen,
                "last_seen": player.last_seen,
                "online": player.online,
            })
        })
        .collect();
    Ok(OkJson(json!({
        "running": running,
        "count": live.as_ref().map_or(online, |status| status.players_online),
        "max": crate::stats::players_max(&row.kind).or(live.map(|status| status.players_max)),
        "known": known,
    })))
}
