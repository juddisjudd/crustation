use std::path::PathBuf;

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::{
    Json, Router,
    routing::{get, post},
};
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
        .route("/{id}/player-actions", post(act))
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
    let (directory, kind) = located(&identity, &state, id).await?;

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

    // So a row can say at a glance who is an operator and who is shut out,
    // rather than making somebody open each list to find out.
    let known_by_id = xuid_names(&directory, &kind).await;
    let mut operators = Vec::new();
    let mut banned = Vec::new();
    let mut listed: Vec<String> = Vec::new();
    for one in lists(&kind) {
        let rows = load_list(&directory, one.file).await.unwrap_or_default();
        let names: Vec<String> = rows
            .iter()
            .filter_map(|row| {
                row.get(one.key)
                    .or_else(|| row.get("name"))
                    .and_then(Value::as_str)
            })
            // A Bedrock permission row holds only an Xbox id, which is nobody's
            // name. Show the person, not the number.
            .map(|found| {
                known_by_id
                    .get(found)
                    .cloned()
                    .unwrap_or_else(|| found.to_string())
            })
            .collect();
        // An address is not somebody, so the banned-ips list stays out of this.
        if one.key != "ip" {
            listed.extend(names.iter().cloned());
        }
        let lowered = names.iter().map(|name| name.to_lowercase()).collect();
        match one.slug {
            "operators" => operators = lowered,
            "banned" => banned = lowered,
            _ => {}
        }
    }
    listed.sort_unstable();
    listed.dedup();

    let status = state.statuses.get(id).await;
    Ok(OkJson(json!({
        "online": status.as_ref().map(|status| &status.sample),
        "count": status.as_ref().map(|status| status.players_online),
        "max": status.as_ref().map(|status| status.players_max),
        // Java sends a sample, not the whole list, so the interface can say so.
        "sampled": kind == "minecraft_java",
        "known": known,
        "lists": lists(&kind).iter().map(|one| one.slug).collect::<Vec<_>>(),
        "operators": operators,
        "banned": banned,
        // Everybody any of the lists names, so the page can act on somebody the
        // server has never seen.
        "listed": listed,
        "running": state.supervisor.state(id).await.is_live(),
        "edition": if kind == "minecraft_bedrock" { "bedrock" } else { "java" },
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

    push_entry(
        &state,
        &identity.user.username,
        &directory,
        &kind,
        &wanted,
        &value,
        body.reason,
        body.level,
    )
    .await?;

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

/// Writes one row into one of the files, in the shape that file expects.
/// Shared by the list editor and by the actions a stopped server cannot run.
#[allow(clippy::too_many_arguments)]
async fn push_entry(
    state: &AppState,
    actor: &str,
    directory: &std::path::Path,
    kind: &str,
    wanted: &Kind,
    value: &str,
    reason: Option<String>,
    level: Option<i64>,
) -> Result<(), ApiError> {
    let value = &as_key(directory, kind, wanted, value).await?;
    let mut rows = load_list(directory, wanted.file).await?;
    if rows
        .iter()
        .any(|row| matches(row, wanted.key, value) || matches(row, "name", value))
    {
        return Err(ApiError::conflict("They are already on that list."));
    }

    let mut entry = json!({ wanted.key: value });

    // Java keys its files by UUID, so look one up rather than write a half row.
    if kind == "minecraft_java" && wanted.key == "name" {
        match profile(state, value).await {
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
            entry["level"] = json!(level.unwrap_or(4).clamp(1, 4));
            entry["bypassesPlayerLimit"] = json!(false);
        }
        "operators" => entry["permission"] = json!("operator"),
        "allow" if kind == "minecraft_bedrock" => entry["ignoresPlayerLimit"] = json!(false),
        "banned" | "banned-ips" => {
            entry["created"] = json!(now);
            entry["source"] = json!(actor);
            entry["expires"] = json!("forever");
            entry["reason"] = json!(reason.unwrap_or_else(|| "Banned by an operator".into()));
        }
        _ => {}
    }

    rows.push(entry);
    save_list(directory, wanted.file, &rows).await
}

/// Takes a row out, whichever of the ways it can be named the operator used.
async fn drop_entry(
    directory: &std::path::Path,
    kind: &str,
    wanted: &Kind,
    value: &str,
) -> Result<bool, ApiError> {
    // Taking a row out by a name it does not carry should still work, so resolve
    // it where we can and fall back to what was asked for where we cannot.
    let resolved = as_key(directory, kind, wanted, value)
        .await
        .unwrap_or_else(|_| value.to_string());
    let value = resolved.as_str();
    let rows = load_list(directory, wanted.file).await?;
    let before = rows.len();
    let kept: Vec<Value> = rows
        .into_iter()
        .filter(|row| {
            !(matches(row, wanted.key, value)
                || matches(row, "name", value)
                || matches(row, "uuid", value))
        })
        .collect();
    let removed = kept.len() != before;
    save_list(directory, wanted.file, &kept).await?;
    Ok(removed)
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

    let value = body.value.trim();
    drop_entry(&directory, &kind, &wanted, value).await?;
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

/// Something to do to one player, from the players page rather than by typing
/// the command out.
#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "kebab-case")]
enum Wanted {
    Op {
        player: String,
    },
    Deop {
        player: String,
    },
    Kick {
        player: String,
        reason: Option<String>,
    },
    Ban {
        player: String,
        reason: Option<String>,
    },
    Pardon {
        player: String,
    },
    /// Java operator level 1 to 4, or a Bedrock permission name.
    Rank {
        player: String,
        rank: String,
    },
    Give {
        player: String,
        item: String,
        count: Option<u32>,
    },
    Teleport {
        player: String,
        to: Spot,
    },
    /// Into chat, as the server.
    Say {
        message: String,
    },
    /// To one player only.
    Whisper {
        player: String,
        message: String,
    },
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Spot {
    Player(String),
    Place { x: f64, y: f64, z: f64 },
}

impl Wanted {
    /// Whose permission it takes. Managing who may play is one thing; reaching
    /// into the game to hand out items or move people about is another.
    fn permission(&self) -> ServerPerm {
        match self {
            Wanted::Give { .. }
            | Wanted::Teleport { .. }
            | Wanted::Say { .. }
            | Wanted::Whisper { .. } => ServerPerm::Commands,
            _ => ServerPerm::Players,
        }
    }

    /// Who it is about, for the lists a stopped server has to be edited through.
    fn subject(&self) -> &str {
        match self {
            Wanted::Op { player }
            | Wanted::Deop { player }
            | Wanted::Kick { player, .. }
            | Wanted::Ban { player, .. }
            | Wanted::Pardon { player }
            | Wanted::Rank { player, .. }
            | Wanted::Give { player, .. }
            | Wanted::Teleport { player, .. }
            | Wanted::Whisper { player, .. } => player.trim(),
            Wanted::Say { .. } => "",
        }
    }
}

/// Commands leave over stdin as one line, so a line break inside an argument
/// would be a second command. Nothing with a control character gets through.
fn safe(field: &'static str, value: &str) -> Result<String, ApiError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(ApiError::field(field, "This cannot be empty."));
    }
    if value.chars().any(char::is_control) {
        return Err(ApiError::field(field, "Take the line breaks out first."));
    }
    Ok(value.to_string())
}

/// Minecraft wants a name with a space in it quoted.
fn argument(value: &str) -> String {
    if value.contains(char::is_whitespace) || value.contains('"') {
        format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        value.to_string()
    }
}

async fn act(
    identity: Identity,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<Wanted>,
) -> ApiResult<impl IntoResponse> {
    identity
        .require_server(&state.db, id, body.permission())
        .await?;
    let row = super::servers::load(&state, id).await?;
    let directory = PathBuf::from(&row.directory);
    let kind = row.kind.as_str();
    let bedrock = kind == "minecraft_bedrock";
    let running = state.supervisor.state(id).await.is_live();

    // What to run in game, and what to write to a file when the server is down.
    // Some of these have no offline half: you cannot kick somebody who is not on.
    let (command, offline): (String, Offline) = match &body {
        Wanted::Op { player } => {
            let name = safe("player", player)?;
            (
                format!("op {}", argument(&name)),
                Offline::Join("operators"),
            )
        }
        Wanted::Deop { player } => {
            let name = safe("player", player)?;
            (
                format!("deop {}", argument(&name)),
                Offline::Leave("operators"),
            )
        }
        Wanted::Kick { player, reason } => {
            let name = safe("player", player)?;
            let reason = match reason {
                Some(text) if !text.trim().is_empty() => format!(" {}", safe("reason", text)?),
                _ => String::new(),
            };
            (format!("kick {}{reason}", argument(&name)), Offline::None)
        }
        Wanted::Ban { player, reason } => {
            if bedrock {
                return Err(ApiError::conflict(
                    "Bedrock has no ban command. Take them off the allow list instead.",
                ));
            }
            let name = safe("player", player)?;
            let text = match reason {
                Some(text) if !text.trim().is_empty() => Some(safe("reason", text)?),
                _ => None,
            };
            let tail = text.as_deref().map(|t| format!(" {t}")).unwrap_or_default();
            (format!("ban {}{tail}", argument(&name)), Offline::Ban(text))
        }
        Wanted::Pardon { player } => {
            if bedrock {
                return Err(ApiError::conflict("Bedrock keeps no ban list."));
            }
            let name = safe("player", player)?;
            (
                format!("pardon {}", argument(&name)),
                Offline::Leave("banned"),
            )
        }
        Wanted::Rank { player, rank } => {
            // Neither edition can change a level in game: Java reads ops.json at
            // start and Bedrock has no permission command. The file is the only
            // way, and it lands on the next start.
            let name = safe("player", player)?;
            let rank = safe("rank", rank)?;
            let changed = rank_in_file(&state, &directory, kind, &name, &rank).await?;
            super::audit(
                &state,
                Some(&identity.user),
                Some(id),
                "changed a player's rank",
                Some(&format!("{name} → {rank}")),
            )
            .await;
            return Ok(OkJson(json!({
                "via": "file",
                "ran": changed,
                "restart_required": running,
            })));
        }
        Wanted::Give {
            player,
            item,
            count,
        } => {
            let name = safe("player", player)?;
            let item = safe("item", item)?;
            if !item
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | ':' | '.' | '-'))
            {
                return Err(ApiError::field("item", "That is not an item id."));
            }
            let count = count.unwrap_or(1).clamp(1, 6400);
            (
                format!("give {} {item} {count}", argument(&name)),
                Offline::None,
            )
        }
        Wanted::Teleport { player, to } => {
            let name = safe("player", player)?;
            let target = match to {
                Spot::Player(other) => argument(&safe("to", other)?),
                Spot::Place { x, y, z } => {
                    for value in [x, y, z] {
                        if !value.is_finite() || value.abs() > 30_000_000.0 {
                            return Err(ApiError::field("to", "That is outside the world."));
                        }
                    }
                    format!("{x} {y} {z}")
                }
            };
            (format!("tp {} {target}", argument(&name)), Offline::None)
        }
        Wanted::Say { message } => (format!("say {}", safe("message", message)?), Offline::None),
        Wanted::Whisper { player, message } => {
            let name = safe("player", player)?;
            (
                format!("tell {} {}", argument(&name), safe("message", message)?),
                Offline::None,
            )
        }
    };

    if !running {
        if matches!(offline, Offline::None) {
            return Err(ApiError::conflict(
                "The server has to be running for that one.",
            ));
        }
        let done = apply_offline(
            &state,
            &identity.user.username,
            &directory,
            kind,
            body.subject(),
            offline,
        )
        .await?;
        super::audit(
            &state,
            Some(&identity.user),
            Some(id),
            "changed a player list while the server was down",
            Some(&command),
        )
        .await;
        return Ok(OkJson(json!({
            "via": "file",
            "ran": done,
            "restart_required": false,
        })));
    }

    let outcome = state
        .supervisor
        .run_command(id, &command)
        .await
        .map_err(|error| ApiError::conflict(error.to_string()))?;
    super::audit(
        &state,
        Some(&identity.user),
        Some(id),
        "acted on a player",
        Some(&command),
    )
    .await;
    Ok(OkJson(json!({
        "via": outcome.via,
        "ran": command,
        "output": outcome.output,
        "restart_required": false,
    })))
}

/// What the same action does when there is no server to talk to.
enum Offline {
    None,
    Join(&'static str),
    Leave(&'static str),
    Ban(Option<String>),
}

/// An Xbox id is a long decimal number, and nobody's gamertag is.
fn is_xuid(value: &str) -> bool {
    value.len() >= 10 && value.chars().all(|c| c.is_ascii_digit())
}

/// Bedrock's allow list is the one file holding a name and an Xbox id side by
/// side, so it is what turns one into the other. Only a player the server has
/// actually seen has an id written down.
async fn xuid_names(
    directory: &std::path::Path,
    kind: &str,
) -> std::collections::HashMap<String, String> {
    let mut found = std::collections::HashMap::new();
    if kind != "minecraft_bedrock" {
        return found;
    }
    for row in load_list(directory, "allowlist.json")
        .await
        .unwrap_or_default()
    {
        let xuid = row.get("xuid").and_then(Value::as_str).unwrap_or_default();
        let name = row.get("name").and_then(Value::as_str).unwrap_or_default();
        if !xuid.is_empty() && !name.is_empty() {
            found.insert(xuid.to_string(), name.to_string());
        }
    }
    found
}

/// The value as that list keys its rows. Bedrock's permissions are keyed by
/// Xbox id, so a name has to be looked up; writing the name straight in makes a
/// row the game quietly ignores.
async fn as_key(
    directory: &std::path::Path,
    kind: &str,
    wanted: &Kind,
    value: &str,
) -> Result<String, ApiError> {
    if wanted.key != "xuid" || is_xuid(value) {
        return Ok(value.to_string());
    }
    let found = xuid_names(directory, kind)
        .await
        .into_iter()
        .find(|(_, name)| name.eq_ignore_ascii_case(value));
    match found {
        Some((xuid, _)) => Ok(xuid),
        None => Err(ApiError::conflict(format!(
            "{} is keyed by Xbox id, and the panel has no id for {value} yet. \
             They get one the first time they connect; until then, start the server \
             and do it in game.",
            wanted.file
        ))),
    }
}

async fn apply_offline(
    state: &AppState,
    actor: &str,
    directory: &std::path::Path,
    kind: &str,
    player: &str,
    offline: Offline,
) -> Result<String, ApiError> {
    let list = |slug| {
        find(kind, slug).ok_or_else(|| {
            ApiError::conflict(
                "This edition does not keep that list, so there is nothing to write.",
            )
        })
    };

    match offline {
        Offline::None => Ok(String::new()),
        Offline::Leave(slug) => {
            let wanted = list(slug)?;
            drop_entry(directory, kind, &wanted, player).await?;
            Ok(format!("took {player} out of {}", wanted.file))
        }
        Offline::Join(slug) => {
            let wanted = list(slug)?;
            push_entry(state, actor, directory, kind, &wanted, player, None, None).await?;
            Ok(format!("wrote {player} into {}", wanted.file))
        }
        Offline::Ban(reason) => {
            let wanted = list("banned")?;
            push_entry(state, actor, directory, kind, &wanted, player, reason, None).await?;
            Ok(format!("wrote {player} into {}", wanted.file))
        }
    }
}

/// Sets a Java operator level or a Bedrock permission, in the file, since
/// neither edition will take it as a command.
async fn rank_in_file(
    state: &AppState,
    directory: &std::path::Path,
    kind: &str,
    name: &str,
    rank: &str,
) -> Result<String, ApiError> {
    let bedrock = kind == "minecraft_bedrock";
    let wanted = find(kind, "operators").ok_or_else(|| ApiError::not_found("List"))?;
    let resolved = as_key(directory, kind, &wanted, name).await?;
    let name = resolved.as_str();

    let value: Value = if bedrock {
        match rank {
            "visitor" | "member" | "operator" => json!(rank),
            _ => {
                return Err(ApiError::field(
                    "rank",
                    "Bedrock knows visitor, member and operator.",
                ));
            }
        }
    } else {
        match rank.parse::<i64>() {
            Ok(level) if (1..=4).contains(&level) => json!(level),
            _ => return Err(ApiError::field("rank", "Java levels run from 1 to 4.")),
        }
    };
    let field = if bedrock { "permission" } else { "level" };

    let mut rows = load_list(directory, wanted.file).await?;
    let found = rows
        .iter_mut()
        .find(|row| matches(row, wanted.key, name) || matches(row, "name", name));

    match found {
        Some(row) => {
            row[field] = value;
        }
        None => {
            // Not on the list yet: put them on it at the rank asked for.
            push_entry(
                state,
                "panel",
                directory,
                kind,
                &wanted,
                name,
                None,
                // Only Java carries a numeric level here.
                if bedrock { None } else { value.as_i64() },
            )
            .await?;
            if bedrock {
                let mut again = load_list(directory, wanted.file).await?;
                if let Some(row) = again
                    .iter_mut()
                    .find(|row| matches(row, wanted.key, name) || matches(row, "name", name))
                {
                    row[field] = value;
                }
                save_list(directory, wanted.file, &again).await?;
            }
            return Ok(format!("added {name} to {}", wanted.file));
        }
    }

    save_list(directory, wanted.file, &rows).await?;
    Ok(format!("set {field} in {}", wanted.file))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bedrock_operators() -> Kind {
        find("minecraft_bedrock", "operators").expect("Bedrock keeps an operator list")
    }

    /// A Bedrock folder with one player the server has seen and one it has not.
    fn sandbox(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("crustation-players-{name}"));
        std::fs::remove_dir_all(&root).ok();
        std::fs::create_dir_all(&root).expect("sandbox");
        std::fs::write(
            root.join("allowlist.json"),
            r#"[{"name":"ohitsjudd","xuid":"2535458356136740","ignoresPlayerLimit":false},
                {"name":"ZoooDuck","ignoresPlayerLimit":false}]"#,
        )
        .expect("allow list");
        root
    }

    #[test]
    fn an_xbox_id_is_a_long_number_and_a_gamertag_is_not() {
        assert!(is_xuid("2535458356136740"));
        assert!(!is_xuid("ohitsjudd"));
        assert!(!is_xuid("12345"));
    }

    #[tokio::test]
    async fn a_name_the_allow_list_knows_becomes_its_xbox_id() {
        let root = sandbox("known");
        let found = as_key(
            &root,
            "minecraft_bedrock",
            &bedrock_operators(),
            "ohitsjudd",
        )
        .await;
        assert_eq!(found.expect("resolved").as_str(), "2535458356136740");
    }

    #[tokio::test]
    async fn a_name_with_no_id_yet_is_refused_rather_than_written() {
        let root = sandbox("unknown");
        let found = as_key(&root, "minecraft_bedrock", &bedrock_operators(), "ZoooDuck").await;
        assert!(found.is_err());
    }

    #[tokio::test]
    async fn an_xbox_id_passes_straight_through() {
        let root = sandbox("passthrough");
        let found = as_key(
            &root,
            "minecraft_bedrock",
            &bedrock_operators(),
            "2535000000000001",
        )
        .await;
        assert_eq!(found.expect("kept").as_str(), "2535000000000001");
    }

    #[tokio::test]
    async fn a_java_list_takes_the_name_as_typed() {
        let root = sandbox("java");
        let ops = find("minecraft_java", "operators").expect("Java keeps an operator list");
        let found = as_key(&root, "minecraft_java", &ops, "Notch").await;
        assert_eq!(found.expect("kept").as_str(), "Notch");
    }

    #[tokio::test]
    async fn an_xbox_id_reads_back_as_the_name_it_belongs_to() {
        let root = sandbox("names");
        let known = xuid_names(&root, "minecraft_bedrock").await;
        assert_eq!(
            known.get("2535458356136740").map(String::as_str),
            Some("ohitsjudd")
        );
    }
}
