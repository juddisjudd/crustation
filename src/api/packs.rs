use std::path::{Path as FsPath, PathBuf};

use axum::body::Body;
use axum::extract::{DefaultBodyLimit, Path, Query, State};
use axum::response::IntoResponse;
use axum::{
    Json, Router,
    routing::{get, post},
};
use futures::StreamExt;
use serde::Deserialize;
use serde_json::json;
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

use crate::auth::Identity;
use crate::error::{ApiError, ApiResult, Ok as OkJson};
use crate::perms::Server as ServerPerm;
use crate::state::AppState;

/// Merged into the servers router, where the id in the path comes from.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/{id}/packs", get(list).post(add))
        .route(
            "/{id}/packs/upload",
            // An add-on is picked in a file dialog and sent straight here, so
            // the browser's own limit is the only one that should apply.
            post(upload).layer(DefaultBodyLimit::disable()),
        )
        .route("/{id}/worlds", get(worlds).put(play))
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
pub(crate) async fn level_name(row: &super::servers::ServerRow, bedrock: bool) -> String {
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

/// The worlds an add-on could be switched on for, so the caller can be asked
/// which when there is more than one.
async fn worlds(
    identity: Identity,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<impl IntoResponse> {
    let (directory, level, bedrock) = located(&identity, &state, id).await?;
    let found = tokio::task::spawn_blocking(move || crate::packs::worlds(&directory, bedrock))
        .await
        .map_err(|error| ApiError::Internal(error.into()))?;
    Ok(OkJson(json!({ "worlds": found, "level_name": level })))
}

#[derive(Deserialize)]
struct Play {
    /// The folder of a world the server already keeps.
    folder: String,
}

/// Points the server at one of the worlds it already has. Minecraft reads
/// `level-name` at startup, so this takes on the next start.
async fn play(
    identity: Identity,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<Play>,
) -> ApiResult<impl IntoResponse> {
    let (directory, _, bedrock) = located(&identity, &state, id).await?;
    if body.folder.trim().is_empty() {
        return Err(ApiError::field("folder", "Name the world to play."));
    }
    let folder = chosen_world(&directory, String::new(), bedrock, Some(body.folder)).await?;

    use_world(&state, id, &folder).await?;
    super::audit(
        &state,
        Some(&identity.user),
        Some(id),
        "changed the world a server plays",
        Some(&folder),
    )
    .await;
    Ok(OkJson(json!({
        "level_name": folder,
        "restart_required": state.supervisor.state(id).await.is_live(),
    })))
}

/// Where an add-on should land, and whether to switch it on once it is there.
#[derive(Deserialize)]
struct Choices {
    /// Whether to tell the world to load it. Bedrock ignores a pack that is
    /// only sitting in the folder.
    #[serde(default = "yes")]
    activate: bool,
    /// Which world to switch it on for. The one `level-name` points at when
    /// nothing is said, which is right whenever there is only one.
    world: Option<String>,
    /// Whether an imported world becomes the one the server plays. Follows
    /// `activate` when nothing is said.
    use_world: Option<bool>,
}

fn yes() -> bool {
    true
}

#[derive(Deserialize)]
struct Add {
    /// A file already in the server folder, uploaded the usual way.
    path: String,
    #[serde(flatten)]
    choices: Choices,
}

/// Settles which world the packs are switched on for. A world named by the
/// caller has to be one the server actually keeps, since it is joined onto a
/// path.
async fn chosen_world(
    directory: &FsPath,
    level: String,
    bedrock: bool,
    asked: Option<String>,
) -> Result<String, ApiError> {
    let Some(asked) = asked
        .map(|one| one.trim().to_string())
        .filter(|one| !one.is_empty())
    else {
        return Ok(level);
    };

    let at = directory.to_path_buf();
    let found = tokio::task::spawn_blocking(move || crate::packs::worlds(&at, bedrock))
        .await
        .map_err(|error| ApiError::Internal(error.into()))?;

    match found.iter().any(|world| world.folder == asked) {
        true => Ok(asked),
        false => Err(ApiError::field(
            "world",
            format!("This server has no world called '{asked}'."),
        )),
    }
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

    place(
        &state,
        &identity,
        id,
        archive,
        &name,
        directory,
        level,
        bedrock,
        body.choices,
    )
    .await
}

/// An add-on or a world chosen in a file dialog, sent straight here rather than
/// dropped in the file browser first. The body is the archive itself.
async fn upload(
    identity: Identity,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(choices): Query<Uploaded>,
    body: Body,
) -> ApiResult<impl IntoResponse> {
    let (directory, level, bedrock) = located(&identity, &state, id).await?;

    let name = choices.name.trim().to_string();
    crate::files::check_name(&name).map_err(|error| ApiError::field("name", error.to_string()))?;
    if !looks_installable(&name) {
        return Err(ApiError::field(
            "name",
            "The panel reads .mcaddon, .mcpack, .mcworld and .zip.",
        ));
    }

    // Outside every server folder, so a half-arrived upload is never something
    // the file browser can show.
    let scratch = state.config.paths.servers.join(".packs");
    tokio::fs::create_dir_all(&scratch).await?;
    let archive = scratch.join(format!("{}-{name}", Uuid::new_v4()));

    let written = receive(&archive, body).await;
    if let Err(error) = written {
        tokio::fs::remove_file(&archive).await.ok();
        return Err(error);
    }

    let result = place(
        &state,
        &identity,
        id,
        archive.clone(),
        &name,
        directory,
        level,
        bedrock,
        choices.choices(),
    )
    .await;
    tokio::fs::remove_file(&archive).await.ok();
    result
}

/// The same choices as the JSON route, spelled out rather than flattened: a
/// query string is not JSON, and `serde` cannot flatten one.
#[derive(Deserialize)]
struct Uploaded {
    /// What the file was called, since the extension says how to read it.
    name: String,
    #[serde(default = "yes")]
    activate: bool,
    world: Option<String>,
    use_world: Option<bool>,
}

impl Uploaded {
    fn choices(&self) -> Choices {
        Choices {
            activate: self.activate,
            world: self.world.clone(),
            use_world: self.use_world,
        }
    }
}

async fn receive(to: &FsPath, body: Body) -> Result<(), ApiError> {
    let mut file = tokio::fs::File::create(to).await?;
    let mut stream = body.into_data_stream();
    while let Some(chunk) = stream.next().await {
        let chunk =
            chunk.map_err(|error| ApiError::validation(format!("The upload stopped: {error}")))?;
        file.write_all(&chunk).await?;
    }
    file.flush().await?;
    Ok(())
}

/// Opens the archive and puts what is inside where the server reads it. Shared
/// by both ways in, so a file dialog and the file browser behave the same.
#[allow(clippy::too_many_arguments)]
async fn place(
    state: &AppState,
    identity: &Identity,
    id: Uuid,
    archive: PathBuf,
    name: &str,
    directory: PathBuf,
    level: String,
    bedrock: bool,
    choices: Choices,
) -> ApiResult<OkJson<serde_json::Value>> {
    if !looks_installable(name) {
        return Err(ApiError::field(
            "path",
            "That is not a pack. The panel reads .mcaddon, .mcpack, .mcworld and .zip.",
        ));
    }

    let level = chosen_world(&directory, level, bedrock, choices.world).await?;

    // Outside the server folder, so a nested pack being opened never turns up
    // in the file browser halfway through.
    let scratch = state.config.paths.servers.join(".packs");
    let activate = choices.activate;
    let into = directory.clone();
    let world = level.clone();
    let installed = tokio::task::spawn_blocking(move || {
        crate::packs::install(&archive, &into, &scratch, &world, bedrock, activate)
    })
    .await
    .map_err(|error| ApiError::Internal(error.into()))?
    .map_err(|error| ApiError::conflict(error.to_string()))?;

    // Importing a world usually means playing it, so the server is pointed at
    // it. The reply says which, since it is a bigger change than dropping in a
    // pack.
    let switched = match installed
        .iter()
        .find(|one| one.sort == crate::packs::Sort::World)
    {
        Some(world) if choices.use_world.unwrap_or(activate) => {
            let folder = world
                .path
                .rsplit('/')
                .next()
                .unwrap_or(&world.path)
                .to_string();
            use_world(state, id, &folder).await?;
            Some(folder)
        }
        _ => None,
    };

    super::audit(
        state,
        Some(&identity.user),
        Some(id),
        "installed an add-on",
        Some(name),
    )
    .await;
    Ok(OkJson(json!({
        "installed": installed,
        "world": level,
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
