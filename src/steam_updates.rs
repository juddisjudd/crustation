//! Keeping Steam servers on the build Steam has. Clients refuse a server on any other build.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Serialize;
use uuid::Uuid;

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
pub fn known(kind: &str, directory: &Path, branch: &str) -> Build {
    let app_id = crate::steam_games::find(kind).map(|title| title.app_id);
    let installed = app_id.and_then(|app| crate::steam::installed_build(directory, app));
    let latest = app_id.and_then(|app| crate::steam::known_latest(app, branch));
    Build {
        branch: branch.to_string(),
        installed,
        latest,
        update_available: matches!((installed, latest), (Some(have), Some(newest)) if have != newest),
    }
}

/// The same, asking Steam if the last answer is more than half an hour old.
pub async fn check(
    state: &AppState,
    kind: &str,
    directory: &Path,
    branch: &str,
) -> anyhow::Result<Build> {
    let Some(title) = crate::steam_games::find(kind) else {
        anyhow::bail!("only a Steam game has a build to check");
    };
    let (program, home) = crate::install::steamcmd(state).await?;
    crate::steam::latest_build(&program, &home, title.app_id, branch).await?;
    Ok(known(kind, directory, branch))
}

/// Asks Steam about every branch a Steam server is on, so the list can say who is behind.
pub async fn watch(state: AppState) {
    tokio::time::sleep(Duration::from_secs(60)).await;
    loop {
        let rows: Vec<(String, String, Option<String>)> =
            sqlx::query_as("SELECT DISTINCT kind, directory, provider_version FROM servers")
                .fetch_all(&state.db)
                .await
                .unwrap_or_default();
        for (kind, directory, branch) in rows {
            if !crate::steam_games::is_steam(&kind) {
                continue;
            }
            let branch = branch.unwrap_or_default();
            if let Err(error) = check(&state, &kind, Path::new(&directory), &branch).await {
                tracing::debug!(%error, %kind, "could not ask Steam for the current build");
            }
        }
        tokio::time::sleep(Duration::from_secs(30 * 60)).await;
    }
}

/// Warns anybody playing, stops, updates and starts again if it was running before.
pub fn spawn_update(
    state: AppState,
    id: Uuid,
    kind: String,
    directory: PathBuf,
    branch: String,
    warn: u64,
) {
    tokio::spawn(async move {
        let was_running = state.supervisor.state(id).await.is_live();
        if was_running {
            crate::scheduler::countdown(&state, id, warn, "restarts for an update").await;
            if let Err(error) = state.supervisor.stop(id).await {
                state.events.notify(
                    "error",
                    format!("Could not stop the server to update it: {error:#}"),
                );
                return;
            }
        }

        state.supervisor.set_flag(id, Flag::Updating, true).await;
        let oxide = kind == crate::rust_game::KIND
            && crate::rust_frameworks::framework_of(&state, id, &directory).await
                == Some(Framework::Oxide);
        let steam_branch = Some(branch.as_str()).filter(|branch| !branch.is_empty());
        let mut result =
            crate::install::update_from_steam(&state, id, &kind, &directory, steam_branch).await;
        // Steam's check puts the game's own assemblies back, which is Oxide gone again.
        if result.is_ok() && oxide {
            state
                .supervisor
                .push_console(id, "install", "Putting Oxide back after the update.".into())
                .await;
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
