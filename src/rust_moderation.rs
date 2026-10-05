//! F7 reports posted to the panel, and a ban list every Rust server on the panel can share.

use anyhow::{Context, Result};
use uuid::Uuid;

use crate::state::AppState;

pub const REPORTS_URL: &str = "server.reportsserverendpoint";
pub const REPORTS_KEY: &str = "server.reportsserverendpointkey";
pub const BANS_URL: &str = "server.bansserverendpoint";
pub const BANS_FAILURE: &str = "server.bansserverfailuremode";

/// The game server runs in the same container, so it reaches the panel on the loopback.
fn panel(state: &AppState) -> String {
    format!(
        "http://127.0.0.1:{}/api/v1/rust-hooks",
        state.config.http.port
    )
}

pub fn reports_url(state: &AppState, id: Uuid) -> String {
    format!("{}/reports/{id}", panel(state))
}

pub fn bans_url(state: &AppState) -> String {
    format!("{}/bans/", panel(state))
}

/// Sets convars on the start command, and on the running server too, since Rust reads
/// these as it needs them rather than once at startup.
pub async fn set(state: &AppState, id: Uuid, changes: Vec<(String, Option<String>)>) -> Result<()> {
    let command: String = sqlx::query_scalar("SELECT command FROM servers WHERE id = ?")
        .bind(id.to_string())
        .fetch_one(&state.db)
        .await?;
    let command = crate::rust_game::with_changes(&command, &changes)
        .context("the start command could not be read")?;
    sqlx::query("UPDATE servers SET command = ?, updated_at = ? WHERE id = ?")
        .bind(&command)
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(id.to_string())
        .execute(&state.db)
        .await?;
    if state.supervisor.state(id).await.is_live() {
        for (key, value) in &changes {
            let value = value.clone().unwrap_or_default();
            state
                .supervisor
                .ask(id, &format!("{key} \"{value}\""))
                .await
                .ok();
        }
    }
    Ok(())
}

/// Turns the report inbox on with a fresh key, or off.
pub async fn use_inbox(state: &AppState, id: Uuid, on: bool) -> Result<()> {
    let changes = match on {
        true => vec![
            (REPORTS_URL.to_string(), Some(reports_url(state, id))),
            (
                REPORTS_KEY.to_string(),
                Some(crate::rcon::generate_password()),
            ),
        ],
        false => vec![
            (REPORTS_URL.to_string(), None),
            (REPORTS_KEY.to_string(), None),
        ],
    };
    set(state, id, changes).await
}

/// Points a server at the panel's ban list, or away; joining stays open if the panel cannot answer.
pub async fn use_shared_bans(state: &AppState, id: Uuid, on: bool) -> Result<()> {
    let changes = match on {
        true => vec![
            (BANS_URL.to_string(), Some(bans_url(state))),
            (BANS_FAILURE.to_string(), Some("0".into())),
        ],
        false => vec![
            (BANS_URL.to_string(), None),
            (BANS_FAILURE.to_string(), None),
        ],
    };
    set(state, id, changes).await
}

/// What Rust wants back for a banned player; `None` means answer 404, not banned.
pub async fn shared_ban(state: &AppState, steam_id: &str) -> Result<Option<serde_json::Value>> {
    let row: Option<(String, Option<i64>, bool)> =
        sqlx::query_as("SELECT reason, expires_at, mute FROM rust_shared_bans WHERE steam_id = ?")
            .bind(steam_id)
            .fetch_optional(&state.db)
            .await?;
    let now = chrono::Utc::now().timestamp();
    Ok(row
        .filter(|(_, expires, _)| expires.is_none_or(|at| at > now))
        .map(|(reason, expires, mute)| {
            serde_json::json!({
                "steamId": steam_id,
                "reason": reason,
                "expiryDate": expires.unwrap_or(0),
                "isMute": mute,
            })
        }))
}
