use std::path::{Path as FsPath, PathBuf};

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::{
    Json, Router,
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::json;
use sqlx::FromRow;
use uuid::Uuid;

use crate::auth::Identity;
use crate::error::{ApiError, ApiResult, Ok as OkJson};
use crate::perms::Server as ServerPerm;
use crate::rust_players::{self, Role};
use crate::state::AppState;
use crate::supervisor::State as ServerState;

use super::servers::ServerRow;

/// Merged into the servers router, where the id in the path comes from.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/{id}/rust/players", get(overview))
        .route("/{id}/rust/player-actions", post(act))
        .route("/{id}/rust/chat", get(chat))
        .route("/{id}/rust/health", get(health))
        .route("/{id}/rust/wipe", post(wipe))
        .route("/{id}/rust/build", get(build).post(update))
}

async fn load_rust(state: &AppState, id: Uuid) -> Result<ServerRow, ApiError> {
    let row = super::servers::load(state, id).await?;
    if row.kind != crate::rust_game::KIND {
        return Err(ApiError::validation("Only Rust servers answer here."));
    }
    Ok(row)
}

async fn read_cfg(path: PathBuf) -> String {
    tokio::fs::read_to_string(path).await.unwrap_or_default()
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
    identity
        .require_server(&state.db, id, ServerPerm::Players)
        .await?;
    let row = load_rust(&state, id).await?;
    let running = state.supervisor.state(id).await.is_live();

    let online = match running {
        true => match state.supervisor.ask(id, "playerlist").await {
            Ok(json) => {
                let online = rust_players::parse_online(&json);
                rust_players::remember(&state.db, id, &online).await;
                online
            }
            Err(_) => Vec::new(),
        },
        false => Vec::new(),
    };

    let rows: Vec<Seen> = sqlx::query_as(
        "SELECT name, uuid, first_seen, last_seen, online FROM server_players
         WHERE server_id = ? ORDER BY online DESC, last_seen DESC",
    )
    .bind(id.to_string())
    .fetch_all(&state.db)
    .await?;
    let known: Vec<_> = rows
        .iter()
        .map(|seen| {
            json!({
                "steam_id": seen.uuid,
                "name": seen.name,
                "first_seen": seen.first_seen,
                "last_seen": seen.last_seen,
                "online": seen.online && running,
            })
        })
        .collect();

    let cfg = rust_players::cfg_dir(FsPath::new(&row.directory), &row.command);
    let staff = rust_players::parse_staff(&read_cfg(cfg.join("users.cfg")).await);
    let live = match running {
        true => state.supervisor.ask(id, "bans").await.ok(),
        false => None,
    };
    let bans = match live.as_deref().and_then(rust_players::parse_live_bans) {
        Some(bans) => bans,
        None => rust_players::parse_bans(&read_cfg(cfg.join("bans.cfg")).await),
    };

    Ok(OkJson(json!({
        "running": running,
        "online": online,
        "known": known,
        "staff": staff,
        "bans": bans,
    })))
}

/// Something to do about one player, or to everybody at once.
#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "kebab-case")]
enum Wanted {
    Kick {
        steam_id: String,
        reason: Option<String>,
    },
    Ban {
        steam_id: String,
        name: Option<String>,
        reason: Option<String>,
        duration: Option<String>,
    },
    Unban {
        steam_id: String,
    },
    /// No role takes both owner and moderator away.
    Staff {
        steam_id: String,
        name: Option<String>,
        role: Option<Role>,
    },
    Mute {
        steam_id: String,
        duration: Option<String>,
    },
    Unmute {
        steam_id: String,
    },
    Say {
        message: String,
    },
    Teleport {
        steam_id: String,
        to: String,
    },
    Give {
        steam_id: String,
        item: String,
        amount: Option<u32>,
    },
    GiveAll {
        item: String,
        amount: Option<u32>,
    },
    Heal {
        steam_id: String,
    },
    HealAll,
    Revive {
        steam_id: String,
    },
    Kill {
        steam_id: String,
    },
    ClearInventory {
        steam_id: String,
    },
}

impl Wanted {
    fn permission(&self) -> ServerPerm {
        match self {
            Wanted::Say { .. }
            | Wanted::Teleport { .. }
            | Wanted::Give { .. }
            | Wanted::GiveAll { .. }
            | Wanted::Heal { .. }
            | Wanted::HealAll
            | Wanted::Revive { .. }
            | Wanted::Kill { .. }
            | Wanted::ClearInventory { .. } => ServerPerm::Commands,
            _ => ServerPerm::Players,
        }
    }
}

fn steam_id(field: &'static str, value: &str) -> Result<String, ApiError> {
    let value = value.trim();
    if !rust_players::is_steam_id(value) {
        return Err(ApiError::field(
            field,
            "That is not a Steam id: 17 digits starting 7656.",
        ));
    }
    Ok(value.to_string())
}

/// Text that goes inside quotes on Rust's console, where a quote would end it early.
fn text(field: &'static str, value: Option<&str>, max: usize) -> Result<String, ApiError> {
    let value = value.unwrap_or_default().trim();
    if value.chars().any(|c| c.is_control() || c == '"') {
        return Err(ApiError::field(
            field,
            "Take out line breaks and double quotes.",
        ));
    }
    if value.chars().count() > max {
        return Err(ApiError::field(
            field,
            format!("Keep it under {max} characters."),
        ));
    }
    Ok(value.to_string())
}

fn short_name(value: &str) -> Result<&str, ApiError> {
    let value = value.trim();
    if !crate::rust_reference::is_short_name(value) {
        return Err(ApiError::field(
            "item",
            "Use the item's short name, like wood.",
        ));
    }
    Ok(value)
}

fn duration(value: Option<&str>) -> Result<Option<String>, ApiError> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    if rust_players::duration_seconds(value).is_none() {
        return Err(ApiError::field(
            "duration",
            "Use hours or days, like 12h or 7d.",
        ));
    }
    Ok(Some(value.to_string()))
}

/// The name the roster has for a Steam id, for commands that want one.
async fn known_name(state: &AppState, id: Uuid, steam_id: &str) -> String {
    sqlx::query_scalar("SELECT name FROM server_players WHERE server_id = ? AND uuid = ? LIMIT 1")
        .bind(id.to_string())
        .bind(steam_id)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten()
        .unwrap_or_default()
}

/// What a stopped server gets instead: a line in one of its cfg files.
struct Offline {
    file: &'static str,
    verbs: &'static [&'static str],
    steam_id: String,
    line: Option<String>,
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
    let row = load_rust(&state, id).await?;
    let running = state.supervisor.state(id).await.is_live();

    // What to run, in order, and the file edit that stands in for it when the server is down.
    let (commands, offline): (Vec<String>, Option<Offline>) = match &body {
        Wanted::Kick {
            steam_id: who,
            reason,
        } => {
            let who = steam_id("steam_id", who)?;
            let reason = text("reason", reason.as_deref(), 128)?;
            (vec![format!("kick {who} \"{reason}\"")], None)
        }
        Wanted::Ban {
            steam_id: who,
            name,
            reason,
            duration: length,
        } => {
            let who = steam_id("steam_id", who)?;
            let mut name = text("name", name.as_deref(), 64)?;
            if name.is_empty() {
                name = known_name(&state, id, &who).await;
            }
            let reason = text("reason", reason.as_deref(), 128)?;
            let length = duration(length.as_deref())?;
            let expiry = length
                .as_deref()
                .and_then(rust_players::duration_seconds)
                .map_or(-1, |seconds| chrono::Utc::now().timestamp() + seconds);
            let mut command = format!("banid {who} \"{name}\" \"{reason}\"");
            if let Some(length) = &length {
                command.push_str(&format!(" {length}"));
            }
            let line = format!("banid {who} \"{name}\" \"{reason}\" {expiry}");
            (
                vec![command],
                Some(Offline {
                    file: "bans.cfg",
                    verbs: &["banid"],
                    steam_id: who,
                    line: Some(line),
                }),
            )
        }
        Wanted::Unban { steam_id: who } => {
            let who = steam_id("steam_id", who)?;
            (
                vec![format!("unban {who}")],
                Some(Offline {
                    file: "bans.cfg",
                    verbs: &["banid"],
                    steam_id: who,
                    line: None,
                }),
            )
        }
        Wanted::Staff {
            steam_id: who,
            name,
            role,
        } => {
            let who = steam_id("steam_id", who)?;
            let mut name = text("name", name.as_deref(), 64)?;
            if name.is_empty() {
                name = known_name(&state, id, &who).await;
            }
            let line = role.map(|role| format!("{} {who} \"{name}\" \"\"", role.command()));
            // One role at a time, so whichever is not wanted is taken away first.
            let mut commands: Vec<String> = [Role::Owner, Role::Moderator]
                .into_iter()
                .filter(|other| Some(*other) != *role)
                .map(|other| format!("{} {who}", other.removal()))
                .collect();
            commands.extend(line.clone());
            (
                commands,
                Some(Offline {
                    file: "users.cfg",
                    verbs: &["ownerid", "moderatorid"],
                    steam_id: who,
                    line,
                }),
            )
        }
        Wanted::Mute {
            steam_id: who,
            duration: length,
        } => {
            let who = steam_id("steam_id", who)?;
            let command = match duration(length.as_deref())? {
                Some(length) => format!("mute {who} {length}"),
                None => format!("mute {who}"),
            };
            (vec![command], None)
        }
        Wanted::Unmute { steam_id: who } => {
            let who = steam_id("steam_id", who)?;
            (vec![format!("unmute {who}")], None)
        }
        Wanted::Say { message } => {
            let message = text("message", Some(message), 256)?;
            if message.is_empty() {
                return Err(ApiError::field("message", "Say something first."));
            }
            (vec![format!("say {message}")], None)
        }
        Wanted::Teleport { steam_id: who, to } => {
            let who = steam_id("steam_id", who)?;
            let to = steam_id("to", to)?;
            (vec![format!("teleport {who} {to}")], None)
        }
        Wanted::Give {
            steam_id: who,
            item,
            amount,
        } => {
            let who = steam_id("steam_id", who)?;
            let item = short_name(item)?;
            let amount = amount.unwrap_or(1).clamp(1, 1_000_000);
            (
                vec![format!("inventory.giveto {who} \"{item}\" {amount}")],
                None,
            )
        }
        Wanted::GiveAll { item, amount } => {
            let item = short_name(item)?;
            let amount = amount.unwrap_or(1).clamp(1, 1_000_000);
            (vec![format!("inventory.giveall \"{item}\" {amount}")], None)
        }
        Wanted::Heal { steam_id: who } => {
            let who = steam_id("steam_id", who)?;
            (vec![format!("debug.refillvitals {who}")], None)
        }
        Wanted::HealAll => (vec!["debug.refillvitalsall".to_string()], None),
        Wanted::Revive { steam_id: who } => {
            let who = steam_id("steam_id", who)?;
            (vec![format!("global.recoverplayer {who}")], None)
        }
        Wanted::Kill { steam_id: who } => {
            let who = steam_id("steam_id", who)?;
            (vec![format!("global.killplayer {who}")], None)
        }
        Wanted::ClearInventory { steam_id: who } => {
            let who = steam_id("steam_id", who)?;
            (vec![format!("inventory.clearinventory {who}")], None)
        }
    };

    if !running {
        let Some(offline) = offline else {
            return Err(ApiError::conflict(
                "The server has to be running for that one.",
            ));
        };
        let cfg = rust_players::cfg_dir(FsPath::new(&row.directory), &row.command);
        tokio::fs::create_dir_all(&cfg).await?;
        let path = cfg.join(offline.file);
        let before = read_cfg(path.clone()).await;
        let after =
            rust_players::with_line(&before, offline.verbs, &offline.steam_id, offline.line);
        tokio::fs::write(&path, after).await?;
        super::audit(
            &state,
            Some(&identity.user),
            Some(id),
            "changed a player list while the server was down",
            Some(&commands.join("; ")),
        )
        .await;
        return Ok(OkJson(json!({ "via": "file", "ran": commands })));
    }

    let mut output = Vec::new();
    for command in &commands {
        let outcome = state
            .supervisor
            .run_command(id, command)
            .await
            .map_err(|error| ApiError::conflict(error.to_string()))?;
        output.extend(outcome.output.filter(|said| !said.is_empty()));
    }
    // Rust holds a ban in memory until something saves the cfg files, so a crash would forget it.
    if offline.as_ref().is_some_and(|edit| edit.file == "bans.cfg") {
        state
            .supervisor
            .run_command(id, "server.writecfg")
            .await
            .ok();
    }
    super::audit(
        &state,
        Some(&identity.user),
        Some(id),
        "acted on a player",
        Some(&commands.join("; ")),
    )
    .await;
    Ok(OkJson(json!({
        "via": "rcon",
        "ran": commands,
        "output": output.join("\n"),
    })))
}

/// Frame rate, entities, queue and network, as the server itself reports them.
async fn health(
    identity: Identity,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<impl IntoResponse> {
    identity
        .require_server(&state.db, id, ServerPerm::Console)
        .await?;
    let row = load_rust(&state, id).await?;
    let now = state.supervisor.state(id).await;
    let live = now.is_live();
    let health = match live {
        true => state
            .supervisor
            .ask(id, "serverinfo")
            .await
            .ok()
            .and_then(|json| rust_players::parse_health(&json)),
        false => None,
    };

    let off = crate::rust_game::convar(&row.command, "app.port").as_deref() == Some("-1");
    let (listening, connects_to) = match live && !off {
        true => state
            .supervisor
            .ask(id, "app.info")
            .await
            .map(|said| rust_players::parse_app_info(&said))
            .unwrap_or_default(),
        false => (None, None),
    };
    let failure = state.supervisor.rust_plus_failure(id).await;
    let rust_plus = json!({
        "state": if off { "off" } else if failure.is_some() { "failed" } else if listening.is_some() { "listening" } else { "unknown" },
        "detail": failure.filter(|said| !said.is_empty()),
        "listening": listening,
        "connects_to": connects_to,
    });
    // A new map's picture is linked just after the start completes, so an older one waits for that.
    let map_image: Option<String> = match now {
        ServerState::Running => {
            sqlx::query_scalar("SELECT map_image FROM servers WHERE id = ?")
                .bind(id.to_string())
                .fetch_one(&state.db)
                .await?
        }
        _ => None,
    };
    Ok(OkJson(json!({
        "health": health,
        "rust_plus": rust_plus,
        "map_image": map_image,
    })))
}

/// What was said lately. Rust keeps it in memory, so a restart starts it again.
async fn chat(
    identity: Identity,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<impl IntoResponse> {
    identity
        .require_server(&state.db, id, ServerPerm::Console)
        .await?;
    load_rust(&state, id).await?;
    let running = state.supervisor.state(id).await.is_live();
    let lines = match running {
        true => state
            .supervisor
            .ask(id, "chat.tail 200")
            .await
            .map(|json| rust_players::parse_chat(&json))
            .unwrap_or_default(),
        false => Vec::new(),
    };
    Ok(OkJson(json!({ "running": running, "lines": lines })))
}

#[derive(Deserialize)]
struct WipeRequest {
    kind: crate::rust_wipe::Wipe,
    #[serde(default)]
    new_seed: bool,
}

async fn wipe(
    identity: Identity,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<WipeRequest>,
) -> ApiResult<impl IntoResponse> {
    identity
        .require_server(&state.db, id, ServerPerm::Config)
        .await?;
    load_rust(&state, id).await?;
    if state.supervisor.state(id).await.is_live() {
        return Err(ApiError::conflict("Stop the server before wiping it."));
    }

    let (deleted, seed) = crate::rust_wipe::wipe_server(&state, id, body.kind, body.new_seed)
        .await
        .map_err(ApiError::Internal)?;

    super::audit(
        &state,
        Some(&identity.user),
        Some(id),
        "wiped the server",
        Some(&format!("{:?}: {} files", body.kind, deleted.len())),
    )
    .await;
    Ok(OkJson(json!({ "deleted": deleted, "seed": seed })))
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
    let row = load_rust(&state, id).await?;
    let directory = PathBuf::from(&row.directory);
    let branch = row.provider_version.clone().unwrap_or_default();
    let build = match crate::rust_updates::check(&state, &directory, &branch).await {
        Ok(build) => build,
        Err(error) => {
            tracing::debug!(%error, "could not ask Steam for the current Rust build");
            crate::rust_updates::known(&directory, &branch)
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
    let row = load_rust(&state, id).await?;
    let runtime = state.supervisor.runtime(id).await;
    if runtime.installing || runtime.updating {
        return Err(ApiError::conflict(
            "This server is already being installed or updated.",
        ));
    }
    let warn = body.warn.min(30 * 60);
    crate::rust_updates::spawn_update(
        state.clone(),
        id,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actions_that_change_the_game_need_the_commands_permission() {
        for json in [
            r#"{"action":"heal","steam_id":"76561198000000001"}"#,
            r#"{"action":"heal-all"}"#,
            r#"{"action":"revive","steam_id":"76561198000000001"}"#,
            r#"{"action":"kill","steam_id":"76561198000000001"}"#,
            r#"{"action":"clear-inventory","steam_id":"76561198000000001"}"#,
        ] {
            let wanted: Wanted = serde_json::from_str(json).expect(json);
            assert!(
                matches!(wanted.permission(), ServerPerm::Commands),
                "{json}"
            );
        }
        let kick: Wanted =
            serde_json::from_str(r#"{"action":"kick","steam_id":"76561198000000001"}"#).unwrap();
        assert!(matches!(kick.permission(), ServerPerm::Players));
    }
}
