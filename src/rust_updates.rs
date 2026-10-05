//! Keeping Rust servers on the build Steam has, which Facepunch forces once a month.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Serialize;
use uuid::Uuid;

use anyhow::Context;

use crate::rust_game::APP_ID;
use crate::rust_mods::Framework;
use crate::state::AppState;
use crate::supervisor::Flag;

#[derive(Serialize)]
pub struct Build {
    pub branch: String,
    pub installed: Option<u64>,
    pub latest: Option<u64>,
    pub update_available: bool,
}

/// What a server has against what Steam has, from the last answer however old.
pub fn known(directory: &Path, branch: &str) -> Build {
    let installed = crate::steam::installed_build(directory, APP_ID);
    let latest = crate::steam::known_latest(APP_ID, branch);
    Build {
        branch: branch.to_string(),
        installed,
        latest,
        update_available: matches!((installed, latest), (Some(have), Some(newest)) if have != newest),
    }
}

/// The same, asking Steam if the last answer is more than half an hour old.
pub async fn check(state: &AppState, directory: &Path, branch: &str) -> anyhow::Result<Build> {
    let (program, home) = crate::install::steamcmd(state).await?;
    crate::steam::latest_build(&program, &home, APP_ID, branch).await?;
    Ok(known(directory, branch))
}

/// Asks Steam about every branch a Rust server is on, so the list can say who is behind.
pub async fn watch(state: AppState) {
    tokio::time::sleep(Duration::from_secs(60)).await;
    loop {
        let rows: Vec<(String, Option<String>)> = sqlx::query_as(
            "SELECT DISTINCT directory, provider_version FROM servers WHERE kind = ?",
        )
        .bind(crate::rust_game::KIND)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();
        for (directory, branch) in rows {
            let branch = branch.unwrap_or_default();
            if let Err(error) = check(&state, Path::new(&directory), &branch).await {
                tracing::debug!(%error, "could not ask Steam for the current Rust build");
            }
        }
        tokio::time::sleep(Duration::from_secs(30 * 60)).await;
    }
}

/// Warns anybody playing, stops, updates and starts again if it was running before.
pub fn spawn_update(state: AppState, id: Uuid, directory: PathBuf, branch: String, warn: u64) {
    tokio::spawn(async move {
        let was_running = state.supervisor.state(id).await.is_live();
        if was_running {
            countdown(&state, id, warn, "restarts for a Rust update").await;
            if let Err(error) = state.supervisor.stop(id).await {
                state.events.notify(
                    "error",
                    format!("Could not stop the server to update it: {error:#}"),
                );
                return;
            }
        }

        state.supervisor.set_flag(id, Flag::Updating, true).await;
        let oxide = framework_of(&state, id, &directory).await == Some(Framework::Oxide);
        let steam_branch = Some(branch.as_str()).filter(|branch| !branch.is_empty());
        let mut result =
            crate::install::update_from_steam(&state, id, &directory, steam_branch).await;
        // Steam's check puts the game's own assemblies back, which is Oxide gone again.
        if result.is_ok() && oxide {
            say(&state, id, "Putting Oxide back after the update.").await;
            result = crate::rust_mods::install_oxide(&state.http, &directory).await;
        }
        state.supervisor.set_flag(id, Flag::Updating, false).await;

        match result {
            Ok(()) if was_running => {
                if let Err(error) = state.supervisor.start_by_id(id).await {
                    state.events.notify(
                        "error",
                        format!("Updated, but could not start again: {error:#}"),
                    );
                }
            }
            Ok(()) => {}
            Err(error) => {
                state
                    .events
                    .notify("error", format!("Could not update the server: {error:#}"));
            }
        }
    });
}

/// Counts down in chat, as in "The server restarts in a minute", then at one minute and ten seconds.
pub async fn countdown(state: &AppState, id: Uuid, seconds: u64, what: &str) {
    let mut marks = vec![seconds, 60, 10];
    marks.retain(|mark| *mark > 0 && *mark <= seconds);
    marks.dedup();
    let mut left = seconds;
    for mark in marks {
        tokio::time::sleep(Duration::from_secs(left - mark)).await;
        left = mark;
        let when = match mark.div_ceil(60) {
            _ if mark < 60 => format!("in {mark} seconds"),
            1 => "in a minute".to_string(),
            minutes => format!("in {minutes} minutes"),
        };
        let message = match what.split_once(' ') {
            Some((verb, reason)) => format!("say The server {verb} {when} {reason}."),
            None => format!("say The server {what} {when}."),
        };
        state.supervisor.run_command(id, &message).await.ok();
    }
    tokio::time::sleep(Duration::from_secs(left)).await;
}

async fn framework_of(state: &AppState, id: Uuid, directory: &Path) -> Option<Framework> {
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
        crate::install::update_from_steam(state, id, directory, branch).await?;
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
