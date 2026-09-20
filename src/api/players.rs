use std::path::PathBuf;

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::{Json, Router, routing::get};
use chrono::Utc;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::FromRow;
use uuid::Uuid;

use crate::auth::Identity;
use crate::error::{ApiError, ApiResult, Done, Ok as OkJson};
use crate::perms::Server as ServerPerm;
use crate::state::AppState;

/// Merged into the servers router, where the id in the path comes from.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/{id}/players", get(overview))
        .route("/{id}/players/{list}", get(read).post(add).delete(remove))
}

/// One of the JSON files a server keeps beside its world.
struct Kind {
    /// What the panel calls it.
    slug: &'static str,
    file: &'static str,
    /// The field that identifies a row, and the one the operator types.
    key: &'static str,
}

fn lists(server_kind: &str) -> Vec<Kind> {
    match server_kind {
        "minecraft_bedrock" => vec![
            Kind {
                slug: "allow",
                file: "allowlist.json",
                key: "name",
            },
            Kind {
                slug: "operators",
                file: "permissions.json",
                // Bedrock keys its permissions by Xbox id, not by name.
                key: "xuid",
            },
        ],
        _ => vec![
            Kind {
                slug: "operators",
                file: "ops.json",
                key: "name",
            },
            Kind {
                slug: "allow",
                file: "whitelist.json",
                key: "name",
            },
            Kind {
                slug: "banned",
                file: "banned-players.json",
                key: "name",
            },
            Kind {
                slug: "banned-ips",
                file: "banned-ips.json",
                key: "ip",
            },
        ],
    }
}

fn find(server_kind: &str, slug: &str) -> Option<Kind> {
    lists(server_kind).into_iter().find(|one| one.slug == slug)
}

async fn located(
    identity: &Identity,
    state: &AppState,
    id: Uuid,
) -> Result<(PathBuf, String), ApiError> {
    identity
        .require_server(&state.db, id, ServerPerm::Players)
        .await?;
    let row = super::servers::load(state, id).await?;
    Ok((PathBuf::from(&row.directory), row.kind))
}

/// Reads one of the files. A file that is not there yet is an empty list, not
/// an error: the server writes it the first time it has something to put in it.
async fn load_list(directory: &std::path::Path, file: &str) -> Result<Vec<Value>, ApiError> {
    let path = directory.join(file);
    let text = match tokio::fs::read_to_string(&path).await {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }
    serde_json::from_str::<Vec<Value>>(&text).map_err(|error| {
        ApiError::conflict(format!("{file} is not a list the panel can read: {error}"))
    })
}

async fn save_list(
    directory: &std::path::Path,
    file: &str,
    rows: &[Value],
) -> Result<(), ApiError> {
    let path = directory.join(file);
    let text =
        serde_json::to_string_pretty(rows).map_err(|error| ApiError::Internal(error.into()))?;
    // Beside and rename, so a crash cannot leave half a file.
    let temporary = path.with_extension("json.tmp");
    tokio::fs::write(&temporary, text.as_bytes()).await?;
    tokio::fs::rename(&temporary, &path).await?;
    Ok(())
}

#[derive(FromRow)]
struct Seen {
    name: String,
    uuid: Option<String>,
    first_seen: String,
    last_seen: String,
    online: bool,
}

async fn overview(
    identity: Identity,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<impl IntoResponse> {
    let (_, kind) = located(&identity, &state, id).await?;

    let rows: Vec<Seen> = sqlx::query_as(
        "SELECT name, uuid, first_seen, last_seen, online FROM server_players
         WHERE server_id = ? ORDER BY online DESC, last_seen DESC",
    )
    .bind(id.to_string())
    .fetch_all(&state.db)
    .await?;

    let known: Vec<Value> = rows
        .iter()
        .map(|row| {
            json!({
                "name": row.name,
                "uuid": row.uuid,
                "first_seen": row.first_seen,
                "last_seen": row.last_seen,
                "online": row.online,
            })
        })
        .collect();

    let status = state.statuses.get(id).await;
    Ok(OkJson(json!({
        "online": status.as_ref().map(|status| &status.sample),
        "count": status.as_ref().map(|status| status.players_online),
        "max": status.as_ref().map(|status| status.players_max),
        // Java sends a sample, not the whole list, so the interface can say so.
        "sampled": kind == "minecraft_java",
        "known": known,
        "lists": lists(&kind).iter().map(|one| one.slug).collect::<Vec<_>>(),
    })))
}

async fn read(
    identity: Identity,
    State(state): State<AppState>,
    Path((id, list)): Path<(Uuid, String)>,
) -> ApiResult<impl IntoResponse> {
    let (directory, kind) = located(&identity, &state, id).await?;
    let wanted = find(&kind, &list).ok_or_else(|| ApiError::not_found("List"))?;
    let rows = load_list(&directory, wanted.file).await?;
    Ok(OkJson(json!({
        "list": wanted.slug,
        "file": wanted.file,
        "key": wanted.key,
        "entries": rows,
    })))
}

#[derive(Deserialize)]
struct Add {
    /// A name, or an address for the banned-ips list.
    value: String,
    reason: Option<String>,
    /// Java operator level, 1 to 4.
    level: Option<i64>,
}

async fn add(
    identity: Identity,
    State(state): State<AppState>,
    Path((id, list)): Path<(Uuid, String)>,
    Json(body): Json<Add>,
) -> ApiResult<impl IntoResponse> {
    let (directory, kind) = located(&identity, &state, id).await?;
    let wanted = find(&kind, &list).ok_or_else(|| ApiError::not_found("List"))?;

    let value = body.value.trim().to_string();
    if value.is_empty() {
        return Err(ApiError::field("value", "Type a name first."));
    }

    let mut rows = load_list(&directory, wanted.file).await?;
    if rows
        .iter()
        .any(|row| matches(row, wanted.key, &value) || matches(row, "name", &value))
    {
        return Err(ApiError::conflict("They are already on that list."));
    }

    let mut entry = json!({ wanted.key: value });

    // Java keys its files by UUID, so look one up rather than write a half row.
    if kind == "minecraft_java" && wanted.key == "name" {
        match profile(&state, &value).await {
            Some((name, uuid)) => {
                entry["name"] = json!(name);
                entry["uuid"] = json!(uuid);
            }
            None => {
                return Err(ApiError::field(
                    "value",
                    "Mojang has no account with that name.",
                ));
            }
        }
    }

    let now = Utc::now().format("%Y-%m-%d %H:%M:%S %z").to_string();
    match wanted.slug {
        "operators" if kind == "minecraft_java" => {
            entry["level"] = json!(body.level.unwrap_or(4).clamp(1, 4));
            entry["bypassesPlayerLimit"] = json!(false);
        }
        "operators" => entry["permission"] = json!("operator"),
        "allow" if kind == "minecraft_bedrock" => entry["ignoresPlayerLimit"] = json!(false),
        "banned" | "banned-ips" => {
            entry["created"] = json!(now);
            entry["source"] = json!(identity.user.username);
            entry["expires"] = json!("forever");
            entry["reason"] = json!(
                body.reason
                    .unwrap_or_else(|| "Banned by an operator".into())
            );
        }
        _ => {}
    }

    rows.push(entry);
    save_list(&directory, wanted.file, &rows).await?;

    super::audit(
        &state,
        Some(&identity.user),
        Some(id),
        &format!("added somebody to {}", wanted.file),
        Some(&value),
    )
    .await;
    Ok(Done)
}

#[derive(Deserialize)]
struct Remove {
    value: String,
}

async fn remove(
    identity: Identity,
    State(state): State<AppState>,
    Path((id, list)): Path<(Uuid, String)>,
    Json(body): Json<Remove>,
) -> ApiResult<impl IntoResponse> {
    let (directory, kind) = located(&identity, &state, id).await?;
    let wanted = find(&kind, &list).ok_or_else(|| ApiError::not_found("List"))?;

    let rows = load_list(&directory, wanted.file).await?;
    let value = body.value.trim();
    // A row goes if the operator named it by any of the ways it can be named.
    let kept: Vec<Value> = rows
        .into_iter()
        .filter(|row| {
            !(matches(row, wanted.key, value)
                || matches(row, "name", value)
                || matches(row, "uuid", value))
        })
        .collect();

    save_list(&directory, wanted.file, &kept).await?;
    super::audit(
        &state,
        Some(&identity.user),
        Some(id),
        &format!("removed somebody from {}", wanted.file),
        Some(value),
    )
    .await;
    Ok(Done)
}

fn matches(row: &Value, field: &str, value: &str) -> bool {
    row.get(field)
        .and_then(Value::as_str)
        .is_some_and(|found| found.eq_ignore_ascii_case(value))
}

/// Mojang's name to UUID lookup, so a row carries the id the server expects.
async fn profile(state: &AppState, name: &str) -> Option<(String, String)> {
    #[derive(Deserialize)]
    struct Profile {
        id: String,
        name: String,
    }

    let found: Profile = state
        .http
        .get(format!(
            "https://api.mojang.com/users/profiles/minecraft/{name}"
        ))
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .json()
        .await
        .ok()?;

    // The API answers without hyphens; the files want them.
    let id = Uuid::parse_str(&found.id).ok()?;
    Some((found.name, id.hyphenated().to_string()))
}
