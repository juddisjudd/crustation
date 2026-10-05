use std::collections::BTreeMap;
use std::path::PathBuf;

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::{Json, Router, routing::get};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::auth::Identity;
use crate::error::{ApiError, ApiResult, Ok as OkJson};
use crate::perms::Server as ServerPerm;
use crate::properties::{self, Properties};
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/", get(list))
}

/// Reading and writing one server's file. Merged into the servers router, since
/// that is where the id in the path comes from.
pub fn server_routes() -> Router<AppState> {
    Router::new().route("/{id}/properties", get(read).put(write))
}

#[derive(Deserialize)]
struct Which {
    kind: String,
}

/// The settings the panel can present for one kind of server. Static reference
/// data, so any signed-in caller may read it.
async fn list(_identity: Identity, Query(which): Query<Which>) -> ApiResult<impl IntoResponse> {
    match which.kind.as_str() {
        "minecraft_java" | "minecraft_bedrock" | crate::rust_game::KIND => {
            Ok(OkJson(properties::catalogue(&which.kind)))
        }
        _ => Err(ApiError::not_found("Server kind")),
    }
}

async fn read(
    identity: Identity,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<impl IntoResponse> {
    identity
        .require_server(&state.db, id, ServerPerm::Config)
        .await?;
    let row = super::servers::load(&state, id).await?;
    if row.kind == crate::rust_game::KIND {
        return Ok(OkJson(rust_settings(&row)));
    }

    let path = properties::path_in(&PathBuf::from(&row.directory));
    let file = Properties::load_if_present(&path)
        .await
        .map_err(ApiError::Internal)?;
    let present: BTreeMap<String, String> = file
        .as_ref()
        .map(|file| file.entries().into_iter().collect())
        .unwrap_or_default();

    // The ones the panel understands, in catalogue order.
    let settings: Vec<Value> = properties::catalogue(&row.kind)
        .iter()
        .map(|entry| {
            let mut value = serde_json::to_value(entry).unwrap_or_default();
            value["value"] = json!(present.get(entry.key).cloned().unwrap_or_default());
            value["set"] = json!(present.contains_key(entry.key));
            value
        })
        .collect();

    // Everything else the file happens to hold, so nothing is hidden.
    let other: Vec<Value> = present
        .iter()
        .filter(|(key, _)| properties::known(&row.kind, key).is_none())
        .map(|(key, value)| {
            json!({
                "key": key,
                "value": value,
                "managed": properties::is_reserved(key),
            })
        })
        .collect();

    Ok(OkJson(json!({
        "exists": file.is_some(),
        "settings": settings,
        "other": other,
    })))
}

#[derive(Deserialize)]
struct Write {
    /// Only the keys to change. Anything left out is untouched.
    #[serde(default)]
    settings: BTreeMap<String, Value>,
}

async fn write(
    identity: Identity,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<Write>,
) -> ApiResult<impl IntoResponse> {
    identity
        .require_server(&state.db, id, ServerPerm::Config)
        .await?;
    let row = super::servers::load(&state, id).await?;

    if body.settings.is_empty() {
        return Err(ApiError::validation("Nothing to change."));
    }
    if row.kind == crate::rust_game::KIND {
        return write_rust(
            &state,
            &identity,
            &row,
            body.settings,
            "changed server settings",
        )
        .await;
    }

    let path = properties::path_in(&PathBuf::from(&row.directory));
    let mut file = match Properties::load_if_present(&path)
        .await
        .map_err(ApiError::Internal)?
    {
        Some(file) => file,
        None => Properties::empty(&path),
    };

    let mut changed = Vec::new();
    for (key, value) in body.settings {
        let field = format!("settings.{key}");
        if properties::is_reserved(&key) {
            return Err(ApiError::field(
                field,
                "The panel writes this one itself; change it on the server instead.",
            ));
        }
        let raw = match value {
            Value::Bool(flag) => flag.to_string(),
            Value::Number(number) => number.to_string(),
            Value::String(text) => text,
            Value::Null => String::new(),
            _ => {
                return Err(ApiError::field(
                    field,
                    "Send text, a number, or true and false.",
                ));
            }
        };

        let checked = match properties::known(&row.kind, &key) {
            Some(entry) => properties::check(entry, &raw),
            None => properties::check_raw(&key, &raw),
        }
        .map_err(|message| ApiError::field(field, message))?;

        file.set(&key, &checked);
        changed.push(key);
    }

    file.save().await.map_err(ApiError::Internal)?;

    crate::api::audit(
        &state,
        Some(&identity.user),
        Some(id),
        "changed server.properties",
        Some(&changed.join(", ")),
    )
    .await;

    Ok(OkJson(json!({
        "changed": changed,
        // Minecraft reads this file once, at startup.
        "restart_required": state.supervisor.state(id).await.is_live(),
    })))
}

/// Rust keeps its settings on the start command, so the screen reads them from
/// there. The RCON password is the one thing left out.
fn rust_settings(row: &super::servers::ServerRow) -> Value {
    let present: BTreeMap<String, String> = crate::rust_game::visible_convars(&row.command)
        .into_iter()
        .collect();

    let settings: Vec<Value> = properties::catalogue(&row.kind)
        .iter()
        .map(|entry| {
            let mut value = serde_json::to_value(entry).unwrap_or_default();
            value["value"] = json!(present.get(entry.key).cloned().unwrap_or_default());
            value["set"] = json!(present.contains_key(entry.key));
            value
        })
        .collect();

    let other: Vec<Value> = present
        .iter()
        .filter(|(key, _)| properties::known(&row.kind, key).is_none())
        .map(|(key, value)| {
            json!({
                "key": key,
                "value": value,
                "managed": crate::rust_game::is_managed(key),
            })
        })
        .collect();

    json!({ "exists": true, "settings": settings, "other": other })
}

pub(crate) async fn write_rust(
    state: &AppState,
    identity: &Identity,
    row: &super::servers::ServerRow,
    requested: BTreeMap<String, Value>,
    action: &str,
) -> ApiResult<OkJson<Value>> {
    let mut changes: Vec<(String, Option<String>)> = Vec::new();
    for (key, value) in requested {
        let field = format!("settings.{key}");
        if crate::rust_game::is_managed(&key) {
            return Err(ApiError::field(
                field,
                "The panel keeps this one in step with the server's port, so it cannot be set here.",
            ));
        }
        let raw = match value {
            Value::Bool(flag) => flag.to_string(),
            Value::Number(number) => number.to_string(),
            Value::String(text) => text,
            Value::Null => String::new(),
            _ => {
                return Err(ApiError::field(
                    field,
                    "Send text, a number, or true and false.",
                ));
            }
        };

        // Empty takes the setting off, which no catalogue rule would accept.
        let checked = match properties::known(&row.kind, &key) {
            Some(_) if raw.trim().is_empty() => Ok(String::new()),
            Some(entry) => properties::check(entry, &raw),
            None => crate::rust_game::check_other(&key, &raw),
        }
        .map_err(|message| ApiError::field(field, message))?;

        // The list needs a name, so an empty one falls back to the panel's.
        let checked = match key == "server.hostname" && checked.is_empty() {
            true => row.name.clone(),
            false => checked,
        };
        changes.push((key, Some(checked)));
    }

    let command = crate::rust_game::with_changes(&row.command, &changes).ok_or_else(|| {
        ApiError::conflict("The start command could not be read, so nothing was changed.")
    })?;
    sqlx::query("UPDATE servers SET command = ?, updated_at = ? WHERE id = ?")
        .bind(&command)
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(&row.id)
        .execute(&state.db)
        .await?;

    let names: Vec<&str> = changes.iter().map(|(key, _)| key.as_str()).collect();
    crate::api::audit(
        state,
        Some(&identity.user),
        Some(row.uuid()),
        action,
        Some(&names.join(", ")),
    )
    .await;

    Ok(OkJson(json!({
        "changed": names,
        // Rust reads its command line once, at startup.
        "restart_required": state.supervisor.state(row.uuid()).await.is_live(),
    })))
}
