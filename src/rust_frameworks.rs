//! Putting Oxide or Carbon on a Rust server, swapping one for the other, and taking it off.

use std::path::{Path, PathBuf};

use anyhow::Context;
use uuid::Uuid;

use crate::rust_mods::Framework;
use crate::state::AppState;
use crate::supervisor::Flag;

pub(crate) async fn framework_of(
    state: &AppState,
    id: Uuid,
    directory: &Path,
) -> Option<Framework> {
    let command: String = sqlx::query_scalar("SELECT command FROM servers WHERE id = ?")
        .bind(id.to_string())
        .fetch_one(&state.db)
        .await
        .ok()?;
    crate::rust_mods::detect(directory, &command)
}

async fn say(state: &AppState, id: Uuid, text: &str) {
    state
        .supervisor
        .push_console(id, "install", text.into())
        .await;
}

/// Puts Oxide or Carbon on a stopped server, takes it off, or swaps one for the other.
pub fn spawn_framework(
    state: AppState,
    id: Uuid,
    directory: PathBuf,
    branch: String,
    target: Option<Framework>,
) {
    tokio::spawn(async move {
        state.supervisor.set_flag(id, Flag::Updating, true).await;
        let result = switch_framework(&state, id, &directory, &branch, target).await;
        state.supervisor.set_flag(id, Flag::Updating, false).await;
        match result {
            Ok(()) => {
                let done = match target {
                    Some(Framework::Oxide) => "Oxide is installed.",
                    Some(Framework::Carbon) => "Carbon is installed.",
                    None => "The server is back to plain Rust.",
                };
                say(&state, id, done).await;
                state.events.notify("success", done.to_string());
            }
            Err(error) => {
                let message = format!("Could not change the plugin framework: {error:#}");
                say(&state, id, &message).await;
                state.events.notify("error", message);
            }
        }
    });
}

async fn switch_framework(
    state: &AppState,
    id: Uuid,
    directory: &Path,
    branch: &str,
    target: Option<Framework>,
) -> anyhow::Result<()> {
    let current = framework_of(state, id, directory).await;
    // Oxide overwrote the game's own assemblies, and only Steam can put them back.
    if current == Some(Framework::Oxide) && target != Some(Framework::Oxide) {
        say(state, id, "Restoring the game files Oxide replaced.").await;
        let branch = Some(branch).filter(|branch| !branch.is_empty());
        crate::install::update_from_steam(state, id, crate::rust_game::KIND, directory, branch)
            .await?;
    }
    if target != Some(Framework::Oxide) {
        crate::rust_mods::remove_oxide(directory).await?;
    }
    match target {
        Some(Framework::Oxide) => {
            say(state, id, "Downloading Oxide.").await;
            crate::rust_mods::install_oxide(&state.http, directory).await?;
        }
        Some(Framework::Carbon) => {
            say(state, id, "Downloading Carbon.").await;
            crate::rust_mods::install_carbon(&state.http, directory, branch).await?;
        }
        None => {}
    }

    let env = match target {
        Some(Framework::Carbon) => crate::rust_mods::carbon_env(directory),
        _ => crate::rust_mods::without_carbon_env(),
    };
    let command: String = sqlx::query_scalar("SELECT command FROM servers WHERE id = ?")
        .bind(id.to_string())
        .fetch_one(&state.db)
        .await?;
    let command = crate::rust_game::with_env(&command, &env)
        .context("the start command has no env to load Carbon through")?;
    sqlx::query("UPDATE servers SET command = ?, updated_at = ? WHERE id = ?")
        .bind(&command)
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(id.to_string())
        .execute(&state.db)
        .await?;
    Ok(())
}
