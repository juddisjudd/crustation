use std::path::PathBuf;

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::{
    Json, Router,
    routing::{get, patch, post},
};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::auth::Identity;
use crate::error::{ApiError, ApiResult, Ok as OkJson};
use crate::perms::Server as ServerPerm;
use crate::rust_mods::{self, Framework};
use crate::state::AppState;

use super::servers::ServerRow;

const UMOD_SEARCH: &str = "https://umod.org/plugins/search.json";

/// Merged into the servers router, where the id in the path comes from.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/{id}/rust/mods", get(overview).put(switch))
        .route("/{id}/rust/plugins", post(upload))
        .route("/{id}/rust/plugins/search", get(search))
        .route("/{id}/rust/plugins/install", post(install))
        .route("/{id}/rust/plugins/updates", get(updates))
        .route("/{id}/rust/plugins/{file}", patch(toggle).delete(remove))
}

async fn load_rust(state: &AppState, id: Uuid) -> Result<ServerRow, ApiError> {
    let row = super::servers::load(state, id).await?;
    if row.kind != crate::rust_game::KIND {
        return Err(ApiError::validation("Only Rust servers answer here."));
    }
    Ok(row)
}

async fn framework(state: &AppState, id: Uuid) -> Result<(ServerRow, Framework), ApiError> {
    let row = load_rust(state, id).await?;
    let found = rust_mods::detect(&PathBuf::from(&row.directory), &row.command)
        .ok_or_else(|| ApiError::conflict("Install Oxide or Carbon first."))?;
    Ok((row, found))
}

/// Tells a running framework what changed, since a file watcher cannot be relied on to.
async fn tell(state: &AppState, id: Uuid, framework: Framework, verb: &str, file: &str) {
    let plugin = file.trim_end_matches(".cs");
    if state.supervisor.state(id).await.is_live() {
        let command = framework.plugin_command(verb, plugin);
        state.supervisor.ask(id, &command).await.ok();
    }
}

fn plugin_file(file: &str) -> Result<String, ApiError> {
    if !rust_mods::is_plugin_file(file) {
        return Err(ApiError::validation(
            "A plugin is one .cs file named with letters, digits and underscores.",
        ));
    }
    Ok(file.to_string())
}

async fn overview(
    identity: Identity,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<impl IntoResponse> {
    identity
        .require_server(&state.db, id, ServerPerm::Files)
        .await?;
    let row = load_rust(&state, id).await?;
    let directory = PathBuf::from(&row.directory);
    let found = rust_mods::detect(&directory, &row.command);
    let running = state.supervisor.state(id).await.is_live();
    let version = match (found, running) {
        (Some(framework), true) => state
            .supervisor
            .ask(id, framework.version_command())
            .await
            .ok()
            .map(|said| said.trim().to_string())
            .filter(|said| !said.is_empty()),
        _ => None,
    };
    let plugins = match found {
        Some(framework) => rust_mods::list(&directory, framework).await,
        None => Vec::new(),
    };
    Ok(OkJson(json!({
        "framework": found,
        "version": version,
        "running": running,
        "plugins": plugins,
    })))
}

#[derive(Deserialize)]
struct Switch {
    framework: Option<Framework>,
}

async fn switch(
    identity: Identity,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<Switch>,
) -> ApiResult<impl IntoResponse> {
    identity
        .require_server(&state.db, id, ServerPerm::Config)
        .await?;
    let row = load_rust(&state, id).await?;
    let runtime = state.supervisor.runtime(id).await;
    if state.supervisor.state(id).await.is_live() {
        return Err(ApiError::conflict(
            "Stop the server before changing how it runs plugins.",
        ));
    }
    if runtime.installing || runtime.updating {
        return Err(ApiError::conflict(
            "This server is already being installed or updated.",
        ));
    }
    if cfg!(windows) {
        return Err(ApiError::conflict(
            "Oxide and Carbon are installed for the Linux server only.",
        ));
    }
    crate::rust_updates::spawn_framework(
        state.clone(),
        id,
        PathBuf::from(&row.directory),
        row.provider_version.clone().unwrap_or_default(),
        body.framework,
    );
    super::audit(
        &state,
        Some(&identity.user),
        Some(id),
        "changed the plugin framework",
        Some(&format!("{:?}", body.framework)),
    )
    .await;
    Ok(OkJson(json!({ "started": true })))
}

#[derive(Deserialize)]
struct Named {
    name: String,
}

/// Takes a plugin as the request body; the framework compiles it on its own when it lands.
async fn upload(
    identity: Identity,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<Named>,
    body: Bytes,
) -> ApiResult<impl IntoResponse> {
    identity
        .require_server(&state.db, id, ServerPerm::Files)
        .await?;
    let (row, framework) = framework(&state, id).await?;
    let file = plugin_file(&query.name)?;
    if std::str::from_utf8(&body).is_err() {
        return Err(ApiError::validation(
            "A plugin is C# source, which is text.",
        ));
    }
    let (enabled, _) = rust_mods::plugin_dirs(&PathBuf::from(&row.directory), framework);
    tokio::fs::create_dir_all(&enabled).await?;
    tokio::fs::write(enabled.join(&file), &body).await?;
    tell(&state, id, framework, "reload", &file).await;
    super::audit(
        &state,
        Some(&identity.user),
        Some(id),
        "uploaded a plugin",
        Some(&file),
    )
    .await;
    Ok(OkJson(json!({ "file": file })))
}

#[derive(Deserialize)]
struct Toggle {
    enabled: bool,
}

/// Moves a plugin in or out of the folder the framework watches, which loads or unloads it.
async fn toggle(
    identity: Identity,
    State(state): State<AppState>,
    Path((id, file)): Path<(Uuid, String)>,
    Json(body): Json<Toggle>,
) -> ApiResult<impl IntoResponse> {
    identity
        .require_server(&state.db, id, ServerPerm::Files)
        .await?;
    let (row, framework) = framework(&state, id).await?;
    let file = plugin_file(&file)?;
    let (enabled, disabled) = rust_mods::plugin_dirs(&PathBuf::from(&row.directory), framework);
    let (from, to) = match body.enabled {
        true => (disabled, enabled),
        false => (enabled, disabled),
    };
    if !from.join(&file).exists() {
        return Err(ApiError::not_found("Plugin"));
    }
    tokio::fs::create_dir_all(&to).await?;
    tokio::fs::rename(from.join(&file), to.join(&file)).await?;
    let verb = if body.enabled { "load" } else { "unload" };
    tell(&state, id, framework, verb, &file).await;
    Ok(OkJson(json!({ "file": file, "enabled": body.enabled })))
}

async fn remove(
    identity: Identity,
    State(state): State<AppState>,
    Path((id, file)): Path<(Uuid, String)>,
) -> ApiResult<impl IntoResponse> {
    identity
        .require_server(&state.db, id, ServerPerm::Files)
        .await?;
    let (row, framework) = framework(&state, id).await?;
    let file = plugin_file(&file)?;
    let (enabled, disabled) = rust_mods::plugin_dirs(&PathBuf::from(&row.directory), framework);
    tell(&state, id, framework, "unload", &file).await;
    let mut gone = false;
    for folder in [enabled, disabled] {
        gone |= tokio::fs::remove_file(folder.join(&file)).await.is_ok();
    }
    if !gone {
        return Err(ApiError::not_found("Plugin"));
    }
    super::audit(
        &state,
        Some(&identity.user),
        Some(id),
        "removed a plugin",
        Some(&file),
    )
    .await;
    Ok(OkJson(json!({ "file": file })))
}

#[derive(Deserialize)]
struct Search {
    #[serde(default)]
    q: String,
    #[serde(default = "first_page")]
    page: u32,
}

fn first_page() -> u32 {
    1
}

async fn umod_search(state: &AppState, query: &str, page: u32) -> Result<Value, ApiError> {
    let url = reqwest::Url::parse_with_params(
        UMOD_SEARCH,
        &[
            ("query", query),
            ("page", &page.to_string()),
            ("sort", "downloads"),
            ("categories", "universal,rust"),
        ],
    )
    .map_err(|error| ApiError::Internal(error.into()))?;
    state
        .http
        .get(url)
        .header("User-Agent", "Crustation")
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|error| ApiError::Unavailable(format!("uMod did not answer: {error}")))?
        .json()
        .await
        .map_err(|error| ApiError::Unavailable(format!("uMod sent something unreadable: {error}")))
}

/// uMod's plugin directory, trimmed to what the page shows.
async fn search(
    identity: Identity,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<Search>,
) -> ApiResult<impl IntoResponse> {
    identity
        .require_server(&state.db, id, ServerPerm::Files)
        .await?;
    load_rust(&state, id).await?;
    let found = umod_search(&state, query.q.trim(), query.page.max(1)).await?;
    let plugins: Vec<Value> = found["data"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .map(|plugin| {
            json!({
                "name": plugin["name"],
                "title": plugin["title"],
                "description": plugin["description"],
                "author": plugin["author"],
                "version": plugin["latest_release_version"],
                "downloads": plugin["downloads"],
                "url": plugin["url"],
            })
        })
        .collect();
    Ok(OkJson(json!({
        "plugins": plugins,
        "page": found["current_page"],
        "last_page": found["last_page"],
        "total": found["total"],
    })))
}

/// Fetches a plugin from uMod by its file name, which also updates one already there.
async fn install(
    identity: Identity,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<Named>,
) -> ApiResult<impl IntoResponse> {
    identity
        .require_server(&state.db, id, ServerPerm::Files)
        .await?;
    let (row, framework) = framework(&state, id).await?;
    let file = plugin_file(&format!("{}.cs", body.name.trim()))?;
    let source = state
        .http
        .get(format!("https://umod.org/plugins/{file}"))
        .header("User-Agent", "Crustation")
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|error| ApiError::Unavailable(format!("uMod did not hand it over: {error}")))?
        .text()
        .await
        .map_err(|error| ApiError::Unavailable(format!("the download was cut short: {error}")))?;
    if rust_mods::read_info(&source).is_none() {
        return Err(ApiError::Unavailable(
            "uMod sent something that is not a plugin.".into(),
        ));
    }

    let (enabled, disabled) = rust_mods::plugin_dirs(&PathBuf::from(&row.directory), framework);
    // An update lands wherever the old copy was, so a switched-off plugin stays off.
    let switched_on = !disabled.join(&file).exists();
    let folder = if switched_on { enabled } else { disabled };
    tokio::fs::create_dir_all(&folder).await?;
    tokio::fs::write(folder.join(&file), source).await?;
    if switched_on {
        tell(&state, id, framework, "reload", &file).await;
    }
    super::audit(
        &state,
        Some(&identity.user),
        Some(id),
        "installed a plugin from uMod",
        Some(&file),
    )
    .await;
    Ok(OkJson(json!({ "file": file })))
}

/// The newest version uMod has of each installed plugin it knows by that name.
async fn updates(
    identity: Identity,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<impl IntoResponse> {
    identity
        .require_server(&state.db, id, ServerPerm::Files)
        .await?;
    let (row, framework) = framework(&state, id).await?;
    let installed = rust_mods::list(&PathBuf::from(&row.directory), framework).await;
    let lookups = installed.iter().map(|plugin| {
        let state = state.clone();
        let name = plugin.name.clone();
        async move {
            let found = umod_search(&state, &name, 1).await.ok()?;
            let latest = found["data"]
                .as_array()?
                .iter()
                .find(|candidate| candidate["name"] == name.as_str())?["latest_release_version"]
                .as_str()?
                .to_string();
            Some((name, latest))
        }
    });
    let latest: serde_json::Map<String, Value> = futures::future::join_all(lookups)
        .await
        .into_iter()
        .flatten()
        .map(|(name, version)| (name, Value::String(version)))
        .collect();
    Ok(OkJson(json!({ "latest": latest })))
}
