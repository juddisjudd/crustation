//! Wiping a Rust server: the save files in its identity folder, chosen by how much goes.

use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Wipe {
    /// The world and everything built in it.
    Map,
    /// What players have learned.
    Blueprints,
    /// Both, plus who everybody was: deaths, identities, teams and clans.
    Full,
}

/// Whether a file goes in this wipe; `companion.id`, `player.tokens`, `cfg/` and emoji never do.
fn goes(name: &str, wipe: Wipe) -> bool {
    let database = |stem: &str| name.starts_with(&format!("{stem}.")) && name.contains(".db");
    let map = name.ends_with(".map")
        || name.ends_with(".sav")
        || name.contains(".sav.")
        || name.ends_with(".navmesh")
        || name.contains("_occlusion_")
        || database("sv.files")
        || database("player.states");
    let blueprints = database("player.blueprints");
    let people = database("player.deaths")
        || database("player.identities")
        || database("relationship")
        || database("clans");
    match wipe {
        Wipe::Map => map,
        Wipe::Blueprints => blueprints,
        Wipe::Full => map || blueprints || people,
    }
}

/// Deletes what the wipe takes out of `identity_dir` and names what went.
pub async fn wipe(identity_dir: &Path, wipe: Wipe) -> Result<Vec<String>> {
    let mut entries = match tokio::fs::read_dir(identity_dir).await {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error).context("reading the save folder"),
    };
    let mut deleted = Vec::new();
    while let Some(entry) = entries.next_entry().await? {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !entry.file_type().await?.is_file() || !goes(&name, wipe) {
            continue;
        }
        tokio::fs::remove_file(entry.path())
            .await
            .with_context(|| format!("deleting {name}"))?;
        deleted.push(name);
    }
    deleted.sort();
    Ok(deleted)
}

/// Wipes a stopped server and, when asked and the map is procedural, writes it a new seed.
pub async fn wipe_server(
    state: &crate::state::AppState,
    id: uuid::Uuid,
    kind: Wipe,
    new_seed: bool,
) -> Result<(Vec<String>, Option<i64>)> {
    let (directory, command): (String, String) =
        sqlx::query_as("SELECT directory, command FROM servers WHERE id = ?")
            .bind(id.to_string())
            .fetch_one(&state.db)
            .await?;
    let folder = crate::rust_players::identity_dir(Path::new(&directory), &command);
    let deleted = wipe(&folder, kind).await?;

    // A custom map comes from its link, so only a procedural one has a seed to change.
    let custom_map = crate::rust_game::visible_convars(&command)
        .iter()
        .any(|(key, value)| key == "server.levelurl" && !value.is_empty());
    let seed = (new_seed && kind != Wipe::Blueprints && !custom_map)
        .then(|| rand::random_range(1..=2_147_483_647_i64));
    if let Some(seed) = seed {
        let changes = [("server.seed".to_string(), Some(seed.to_string()))];
        let command = crate::rust_game::with_changes(&command, &changes)
            .context("the start command could not be read, so the seed was left alone")?;
        sqlx::query("UPDATE servers SET command = ?, updated_at = ? WHERE id = ?")
            .bind(&command)
            .bind(chrono::Utc::now().to_rfc3339())
            .bind(id.to_string())
            .execute(&state.db)
            .await?;
    }
    Ok((deleted, seed))
}

#[cfg(test)]
mod tests {
    use super::*;

    const FOLDER: [&str; 21] = [
        "Log.EAC.txt",
        "clans.289.db",
        "clans.289.db-wal",
        "companion.id",
        "player.blueprints.18.db",
        "player.blueprints.18.db-wal",
        "player.deaths.18.db",
        "player.identities.18.db",
        "player.states.289.db",
        "player.states.289.db-wal",
        "player.tokens.db",
        "player.tokens.db-wal",
        "proceduralmap.1000.1337.289.map",
        "proceduralmap.1000.1337.289.navmesh",
        "proceduralmap.1000.1337.289.sav",
        "proceduralmap.1000.1337.289.sav.1",
        "proceduralmap.1000.1337.289_occlusion_3.dat",
        "relationship.289.db",
        "sv.files.289.db",
        "sv.files.289.db-wal",
        "serverauto.cfg",
    ];

    fn kept(wipe: Wipe) -> Vec<&'static str> {
        FOLDER
            .into_iter()
            .filter(|name| !goes(name, wipe))
            .collect()
    }

    #[test]
    fn a_map_wipe_keeps_blueprints_and_people() {
        let kept = kept(Wipe::Map);
        assert!(kept.contains(&"player.blueprints.18.db"));
        assert!(kept.contains(&"player.identities.18.db"));
        assert!(!kept.contains(&"proceduralmap.1000.1337.289.sav.1"));
        assert!(!kept.contains(&"sv.files.289.db-wal"));
    }

    #[test]
    fn a_blueprint_wipe_takes_only_blueprints() {
        let gone: Vec<_> = FOLDER
            .into_iter()
            .filter(|name| goes(name, Wipe::Blueprints))
            .collect();
        assert_eq!(
            gone,
            ["player.blueprints.18.db", "player.blueprints.18.db-wal"]
        );
    }

    #[test]
    fn a_full_wipe_leaves_pairings_and_settings() {
        assert_eq!(
            kept(Wipe::Full),
            [
                "Log.EAC.txt",
                "companion.id",
                "player.tokens.db",
                "player.tokens.db-wal",
                "serverauto.cfg"
            ]
        );
    }

    #[tokio::test]
    async fn deletes_only_what_the_wipe_names() {
        let folder = std::env::temp_dir().join("crustation-rust-wipe");
        tokio::fs::remove_dir_all(&folder).await.ok();
        tokio::fs::create_dir_all(folder.join("cfg")).await.unwrap();
        for name in [
            "proceduralmap.1000.1.289.sav",
            "companion.id",
            "player.blueprints.18.db",
        ] {
            tokio::fs::write(folder.join(name), b"x").await.unwrap();
        }
        let deleted = wipe(&folder, Wipe::Map).await.unwrap();
        assert_eq!(deleted, ["proceduralmap.1000.1.289.sav"]);
        assert!(folder.join("companion.id").exists());
        assert!(folder.join("cfg").exists());
        tokio::fs::remove_dir_all(&folder).await.ok();
    }
}
