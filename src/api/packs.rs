use std::path::{Path as FsPath, PathBuf};

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::{Json, Router, routing::get};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::auth::Identity;
use crate::error::{ApiError, ApiResult, Ok as OkJson};
use crate::perms::Server as ServerPerm;
use crate::state::AppState;

/// Merged into the servers router, where the id in the path comes from.
pub fn routes() -> Router<AppState> {
    Router::new().route("/{id}/packs", get(list).post(add))
}

/// The extensions the game itself uses, plus the plain zip plenty of people
/// hand out instead.
const TAKEN: [&str; 4] = [".mcaddon", ".mcpack", ".mcworld", ".zip"];

pub fn looks_installable(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    TAKEN.iter().any(|one| lower.ends_with(one))
}

async fn located(
    identity: &Identity,
    state: &AppState,
    id: Uuid,
) -> Result<(PathBuf, String, bool), ApiError> {
    identity
        .require_server(&state.db, id, ServerPerm::Files)
        .await?;
    let row = super::servers::load(state, id).await?;
    let bedrock = row.kind == "minecraft_bedrock";
    let directory = PathBuf::from(&row.directory);
    Ok((directory, level_name(&row, bedrock).await, bedrock))
}

/// The world the packs would be switched on for, as `server.properties` names
/// it. Each edition falls back to its own default.
async fn level_name(row: &super::servers::ServerRow, bedrock: bool) -> String {
    let default = if bedrock { "Bedrock level" } else { "world" };
    crate::properties::Properties::load_if_present(crate::properties::path_in(FsPath::new(
        &row.directory,
    )))
    .await
    .ok()
    .flatten()
    .and_then(|file| file.get("level-name"))
    .map(|found| found.trim().to_string())
    .filter(|found| !found.is_empty())
    .unwrap_or_else(|| default.to_string())
}

async fn list(
    identity: Identity,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<impl IntoResponse> {
    let (directory, level, bedrock) = located(&identity, &state, id).await?;
    let found =
        tokio::task::spawn_blocking(move || crate::packs::installed(&directory, &level, bedrock))
            .await
            .map_err(|error| ApiError::Internal(error.into()))?;
    Ok(OkJson(json!({ "packs": found })))
}

#[derive(Deserialize)]
struct Add {
    /// A file already in the server folder, uploaded the usual way.
    path: String,
    /// Whether to tell the world to load it. Bedrock ignores a pack that is
    /// only sitting in the folder.
    #[serde(default = "yes")]
    activate: bool,
}

fn yes() -> bool {
    true
}

async fn add(
    identity: Identity,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<Add>,
) -> ApiResult<impl IntoResponse> {
    let (directory, level, bedrock) = located(&identity, &state, id).await?;

    let archive = crate::files::resolve(&directory, &body.path)
        .map_err(|refused| ApiError::field("path", refused.to_string()))?;
    if !archive.is_file() {
        return Err(ApiError::not_found("File"));
    }
    let name = archive
        .file_name()
        .and_then(|one| one.to_str())
        .unwrap_or_default()
        .to_string();
    if !looks_installable(&name) {
        return Err(ApiError::field(
            "path",
            "That is not a pack. The panel reads .mcaddon, .mcpack and .zip.",
        ));
    }

    // Outside the server folder, so a nested pack being opened never turns up
    // in the file browser halfway through.
    let scratch = state.config.paths.servers.join(".packs");
    let activate = body.activate;
    let installed = tokio::task::spawn_blocking(move || {
        crate::packs::install(&archive, &directory, &scratch, &level, bedrock, activate)
    })
    .await
    .map_err(|error| ApiError::Internal(error.into()))?
    .map_err(|error| ApiError::conflict(error.to_string()))?;

    // Importing a world means playing it, so the server is pointed at it. The
    // reply says which, since it is a bigger change than dropping in a pack.
    let switched = match installed
        .iter()
        .find(|one| one.sort == crate::packs::Sort::World)
    {
        Some(world) if activate => {
            let folder = world
                .path
                .rsplit('/')
                .next()
                .unwrap_or(&world.path)
                .to_string();
            use_world(&state, id, &folder).await?;
            Some(folder)
        }
        _ => None,
    };

    super::audit(
        &state,
        Some(&identity.user),
        Some(id),
        "installed an add-on",
        Some(&name),
    )
    .await;
    Ok(OkJson(json!({
        "installed": installed,
        "level_name": switched,
        "restart_required": true,
    })))
}

/// Points `level-name` at a world that has just arrived.
async fn use_world(state: &AppState, id: Uuid, folder: &str) -> Result<(), ApiError> {
    let row = super::servers::load(state, id).await?;
    let path = crate::properties::path_in(FsPath::new(&row.directory));
    let mut file = crate::properties::Properties::load_if_present(&path)
        .await
        .map_err(ApiError::Internal)?
        .unwrap_or_else(|| crate::properties::Properties::empty(&path));
    file.set("level-name", folder);
    file.save().await.map_err(ApiError::Internal)?;
    Ok(())
}
