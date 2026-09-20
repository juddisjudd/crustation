use std::path::{Path as FsPath, PathBuf};

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::{Json, Router, routing::get};
use base64::Engine as _;
use chrono::Utc;
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::auth::Identity;
use crate::bridge::{Happening, Spot};
use crate::error::{ApiError, ApiResult, Done, Ok as OkJson};
use crate::perms::Server as ServerPerm;
use crate::state::AppState;

/// The uuid in the add-on's manifest, and the uuid of the script module inside
/// it. Bedrock keys a pack's own settings folder by the script module's uuid,
/// not the pack's, so both are needed.
const PACK_UUID: &str = "6b0c8a54-7f2a-4a2e-9a5a-2f1d6c3b8e10";
const SCRIPT_UUID: &str = "6b0c8a54-7f2a-4a2e-9a5a-2f1d6c3b8e11";
const FOLDER: &str = "crustation";

/// What the add-on needs and the default file does not list.
const NEEDED_MODULES: [&str; 2] = ["@minecraft/server-net", "@minecraft/server-admin"];

/// Merged into the servers router for the managed half; the half the add-on
/// itself calls is mounted separately, since a token is its only credential.
pub fn routes() -> Router<AppState> {
    Router::new().route("/{id}/bridge", get(status).post(install).delete(uninstall))
}

/// Not nested under `/servers`, and deliberately without an `Identity`: the
/// caller is a game server, not a person, and the token in the path is what
/// stands in for a session.
pub fn public_routes() -> Router<AppState> {
    Router::new().route("/{token}", axum::routing::post(exchange))
}

/// Where the add-on is kept in the image. Overridable so `cargo run` finds it.
fn addon_dir() -> PathBuf {
    std::env::var("CRUSTATION_ADDONS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("addons"))
        .join("crustation-bedrock")
}

#[derive(Deserialize)]
struct CheckIn {
    #[serde(default)]
    events: Vec<Happening>,
    #[serde(default)]
    players: Vec<Spot>,
    /// Every item id the running server knows. Sent only when the reply to the
    /// last check-in asked for it, since it is long and never changes on its
    /// own.
    #[serde(default)]
    items: Option<Vec<String>>,
}

/// The add-on checking in: here is what happened, what should I run?
async fn exchange(
    State(state): State<AppState>,
    Path(token): Path<String>,
    Json(body): Json<CheckIn>,
) -> ApiResult<impl IntoResponse> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT server_id FROM server_bridge WHERE token = ?")
            .bind(&token)
            .fetch_optional(&state.db)
            .await?;
    // An unknown token is a 404 rather than a 401, so probing it tells nobody
    // whether any given token happens to exist.
    let Some((server_id,)) = row else {
        return Err(ApiError::not_found("Bridge"));
    };
    let id = Uuid::parse_str(&server_id).map_err(|_| ApiError::not_found("Bridge"))?;

    for event in &body.events {
        // Into the console as the Java server would write it, so the chat tab,
        // the search and everything else already suits it.
        state
            .supervisor
            .push_console(id, "stdout", event.as_console_line())
            .await;
        if let Some(name) = event.player() {
            remember_player(&state, id, name, matches!(event, Happening::Leave { .. })).await;
        }
    }

    if let Some(items) = body.items {
        state.bridges.took_items(id, items).await;
    }
    state.bridges.arrived(id, body.players).await;
    Ok(OkJson(
        json!({ "want_items": state.bridges.wants_items(id).await }),
    ))
}

/// Bedrock's ping carries no player list at all, so who is on is only known
/// because the add-on said so.
async fn remember_player(state: &AppState, id: Uuid, name: &str, leaving: bool) {
    let now = Utc::now().to_rfc3339();
    let result = sqlx::query(
        "INSERT INTO server_players (server_id, name, uuid, first_seen, last_seen, online)
         VALUES (?, ?, NULL, ?, ?, ?)
         ON CONFLICT(server_id, name) DO UPDATE SET last_seen = excluded.last_seen,
                                                    online = excluded.online",
    )
    .bind(id.to_string())
    .bind(name)
    .bind(&now)
    .bind(&now)
    .bind(!leaving)
    .execute(&state.db)
    .await;
    if let Err(error) = result {
        tracing::debug!(%error, "could not remember a player the add-on named");
    }
}

async fn located(
    identity: &Identity,
    state: &AppState,
    id: Uuid,
    permission: ServerPerm,
) -> Result<(PathBuf, String), ApiError> {
    identity.require_server(&state.db, id, permission).await?;
    let row = super::servers::load(state, id).await?;
    if row.kind != "minecraft_bedrock" {
        return Err(ApiError::conflict(
            "Only Bedrock needs the add-on. Java servers get all this over RCON.",
        ));
    }
    let directory = PathBuf::from(&row.directory);
    let level = crate::properties::Properties::load_if_present(crate::properties::path_in(
        FsPath::new(&row.directory),
    ))
    .await
    .ok()
    .flatten()
    .and_then(|file| file.get("level-name"))
    .map(|found| found.trim().to_string())
    .filter(|found| !found.is_empty())
    .unwrap_or_else(|| "Bedrock level".to_string());
    Ok((directory, level))
}

async fn status(
    identity: Identity,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<impl IntoResponse> {
    let (directory, level) = located(&identity, &state, id, ServerPerm::Console).await?;
    let installed: Option<(String,)> =
        sqlx::query_as("SELECT token FROM server_bridge WHERE server_id = ?")
            .bind(id.to_string())
            .fetch_optional(&state.db)
            .await?;

    let suggested = state
        .config
        .http
        .public_url
        .clone()
        .unwrap_or_else(|| format!("http://127.0.0.1:{}", state.config.http.port));

    let world = directory.join("worlds").join(&level);
    let beta = tokio::task::spawn_blocking(move || beta_apis_on(&world))
        .await
        .unwrap_or(None);

    Ok(OkJson(json!({
        "supported": true,
        "installed": installed.is_some(),
        "connected": state.bridges.connected(id).await,
        "last_seen": state.bridges.last_seen(id).await,
        // None when there is no world yet, which is not the same as off.
        "beta_apis": beta,
        "world": level,
        "suggested_url": suggested,
        "notes": notes(&state, id).await,
    })))
}

/// What the server said about the add-on, pulled out of the console.
///
/// When a script will not load, the game says so once at startup and then
/// never again, and it is easily lost in a few hundred lines of world loading.
/// This is the difference between "it does not work" and a reason.
async fn notes(state: &AppState, id: Uuid) -> Vec<String> {
    const WORTH_SAYING: [&str; 7] = [
        "crustation",
        "script",
        "module",
        "@minecraft",
        "scripting",
        "was not found and was ignored",
        "beta api",
    ];
    // A Bedrock server lists every pack it loaded at startup, one line each. The
    // only one that answers "is the add-on loaded" is the one naming it, and it
    // is caught by "crustation" above, so the rest are dropped rather than
    // filling a card in the sidebar with a stack nobody asked about.
    const NOISE: [&str; 1] = ["pack stack - ["];

    let lines = state.supervisor.console(id, None).await;
    let mut found: Vec<String> = Vec::new();

    for line in lines.iter() {
        let lower = line.text.to_lowercase();
        let worth = WORTH_SAYING.iter().any(|one| lower.contains(one));
        let noise = NOISE
            .iter()
            .any(|one| lower.contains(one) && !lower.contains("crustation"));
        if !worth || noise {
            continue;
        }
        // A server that has been restarted says the same things again, and two
        // copies of one complaint reads as two problems.
        if !found.contains(&line.text) {
            found.push(line.text.clone());
        }
    }

    // The last few are the ones from this start, which is the run being asked
    // about.
    let from = found.len().saturating_sub(8);
    found.drain(..from);
    found
}

fn beta_apis_on(world: &FsPath) -> Option<bool> {
    let bytes = std::fs::read(world.join("level.dat")).ok()?;
    let level = crate::nbt::parse(&bytes).ok()?;
    Some(crate::nbt::experiment_on(&level, crate::nbt::BETA_APIS))
}

#[derive(Deserialize, Default)]
struct Install {
    /// The address the game server should call back on. The panel cannot work
    /// this out for itself: it sees the address the browser used, which may not
    /// be the one the server can reach.
    panel_url: Option<String>,
}

async fn install(
    identity: Identity,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    body: Option<Json<Install>>,
) -> ApiResult<impl IntoResponse> {
    let (directory, level) = located(&identity, &state, id, ServerPerm::Config).await?;
    let Json(body) = body.unwrap_or_default();

    let panel_url = body
        .panel_url
        .map(|one| one.trim().trim_end_matches('/').to_string())
        .filter(|one| !one.is_empty())
        .or_else(|| {
            state
                .config
                .http
                .public_url
                .clone()
                .map(|one| one.trim_end_matches('/').to_string())
        })
        // The panel and the game server share a container in every supported
        // deployment, so loopback is right far more often than not.
        .unwrap_or_else(|| format!("http://127.0.0.1:{}", state.config.http.port));

    let source = addon_dir();
    if !source.join("manifest.json").is_file() {
        return Err(ApiError::conflict(
            "The add-on is missing from this build of the panel.",
        ));
    }

    // A fresh token every install, so removing and adding it again shuts the
    // old one out.
    let mut raw = vec![0u8; 32];
    rand::fill(&mut raw[..]);
    let token = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&raw);

    let where_to = directory.clone();
    let world = directory.join("worlds").join(&level);
    let url = panel_url.clone();
    let secret = token.clone();
    let laid =
        tokio::task::spawn_blocking(move || lay_it_down(&source, &where_to, &world, &url, &secret))
            .await
            .map_err(|error| ApiError::Internal(error.into()))?
            .map_err(|error| ApiError::conflict(error.to_string()))?;

    sqlx::query(
        "INSERT INTO server_bridge (server_id, token, installed_at) VALUES (?, ?, ?)
         ON CONFLICT(server_id) DO UPDATE SET token = excluded.token,
                                              installed_at = excluded.installed_at",
    )
    .bind(id.to_string())
    .bind(&token)
    .bind(Utc::now().to_rfc3339())
    .execute(&state.db)
    .await?;

    super::audit(
        &state,
        Some(&identity.user),
        Some(id),
        "installed the Bedrock add-on",
        Some(&panel_url),
    )
    .await;

    Ok(OkJson(json!({
        "panel_url": panel_url,
        "world": level,
        "beta_apis_turned_on": laid.turned_beta_on,
        "world_missing": laid.world_missing,
        "restart_required": true,
    })))
}

struct Laid {
    turned_beta_on: bool,
    /// The world is made on first start, so installing before then is fine as
    /// long as somebody knows to install again after.
    world_missing: bool,
}

/// Puts the add-on in place: the pack itself, the world's pack list, the module
/// allow list and the settings the script reads.
fn lay_it_down(
    source: &FsPath,
    server: &FsPath,
    world: &FsPath,
    panel_url: &str,
    token: &str,
) -> anyhow::Result<Laid> {
    let into = server.join("behavior_packs").join(FOLDER);
    std::fs::remove_dir_all(&into).ok();
    std::fs::create_dir_all(&into)?;
    crate::files::copy_tree(source, &into)?;

    // Bedrock will not load a module the allow list does not name, and the
    // file it reads is shared with every other pack, so add rather than replace.
    let permissions = server
        .join("config")
        .join("default")
        .join("permissions.json");
    std::fs::create_dir_all(permissions.parent().unwrap_or(server))?;
    let mut allowed: Vec<String> = std::fs::read_to_string(&permissions)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|parsed| parsed.get("allowed_modules").cloned())
        .and_then(|list| serde_json::from_value(list).ok())
        .unwrap_or_else(|| {
            [
                "@minecraft/server",
                "@minecraft/server-ui",
                "@minecraft/server-gametest",
            ]
            .map(str::to_string)
            .to_vec()
        });
    for module in NEEDED_MODULES {
        if !allowed.iter().any(|one| one == module) {
            allowed.push(module.to_string());
        }
    }
    write_json(
        &permissions,
        &serde_json::json!({ "allowed_modules": allowed }),
    )?;

    // Keyed by the script module's uuid, which is where the script looks.
    let settings = server.join("config").join(SCRIPT_UUID);
    std::fs::create_dir_all(&settings)?;
    write_json(
        &settings.join("variables.json"),
        &serde_json::json!({ "panelUrl": panel_url, "token": token }),
    )?;

    let mut laid = Laid {
        turned_beta_on: false,
        world_missing: !world.is_dir(),
    };
    if laid.world_missing {
        return Ok(laid);
    }

    // The world has to load the pack, and the pack needs the experiment on, or
    // the script module is never given @minecraft/server-net at all.
    let list = world.join("world_behavior_packs.json");
    let mut rows = crate::packs::read_world_list(&list)?;
    if !rows
        .iter()
        .any(|row| row.get("pack_id").and_then(serde_json::Value::as_str) == Some(PACK_UUID))
    {
        rows.push(serde_json::json!({ "pack_id": PACK_UUID, "version": [1, 0, 0] }));
    }
    crate::packs::write_world_list(&list, &rows)?;

    let level_dat = world.join("level.dat");
    if let Ok(bytes) = std::fs::read(&level_dat) {
        let mut level = crate::nbt::parse(&bytes)?;
        if !crate::nbt::experiment_on(&level, crate::nbt::BETA_APIS) {
            crate::nbt::set_experiment(&mut level, crate::nbt::BETA_APIS, true)?;
            // Beside and rename: a half-written level.dat is a lost world.
            let temporary = level_dat.with_extension("dat.tmp");
            std::fs::write(&temporary, crate::nbt::write(&level))?;
            std::fs::rename(&temporary, &level_dat)?;
            laid.turned_beta_on = true;
        }
    }

    Ok(laid)
}

fn write_json(path: &FsPath, value: &serde_json::Value) -> anyhow::Result<()> {
    let text = serde_json::to_string_pretty(value)?;
    let temporary = path.with_extension("tmp");
    std::fs::write(&temporary, text)?;
    std::fs::rename(&temporary, path)?;
    Ok(())
}

async fn uninstall(
    identity: Identity,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<impl IntoResponse> {
    let (directory, _) = located(&identity, &state, id, ServerPerm::Config).await?;

    let folder = directory.join("behavior_packs").join(FOLDER);
    let worlds = directory.join("worlds");
    tokio::task::spawn_blocking(move || {
        std::fs::remove_dir_all(&folder).ok();
        // Every world, not just the one running: a world left pointing at a
        // pack that is no longer there is a world that will not load.
        let Ok(entries) = std::fs::read_dir(&worlds) else {
            return;
        };
        for entry in entries.flatten() {
            let list = entry.path().join("world_behavior_packs.json");
            let Ok(text) = std::fs::read_to_string(&list) else {
                continue;
            };
            let Ok(rows) = serde_json::from_str::<Vec<serde_json::Value>>(&text) else {
                continue;
            };
            let kept: Vec<serde_json::Value> = rows
                .into_iter()
                .filter(|row| {
                    row.get("pack_id").and_then(serde_json::Value::as_str) != Some(PACK_UUID)
                })
                .collect();
            write_json(&list, &serde_json::Value::Array(kept)).ok();
        }
    })
    .await
    .map_err(|error| ApiError::Internal(error.into()))?;

    sqlx::query("DELETE FROM server_bridge WHERE server_id = ?")
        .bind(id.to_string())
        .execute(&state.db)
        .await?;
    state.bridges.forget(id).await;

    super::audit(
        &state,
        Some(&identity.user),
        Some(id),
        "removed the Bedrock add-on",
        None,
    )
    .await;
    Ok(Done)
}
