//! What somebody wrote down about an add-on.
//!
//! Plenty of packs need something run in the game before they do anything, or
//! have a command that puts them right when they misbehave, and the pack
//! itself is the only place that says so. A note keeps that beside the pack in
//! the panel, with the commands as buttons rather than something to remember.
//!
//! Kept per server and keyed by the pack's id, so updating a pack or
//! reinstalling it into another folder does not lose what was written about it.

use std::collections::HashMap;

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::{Json, Router, routing::get};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::FromRow;
use uuid::Uuid;

use crate::auth::Identity;
use crate::error::{ApiError, ApiResult, Ok as OkJson};
use crate::perms::Server as ServerPerm;
use crate::state::AppState;

/// Merged into the servers router, where the id in the path comes from. The
/// pack's id is its own segment rather than a query, so the path says what the
/// note is about.
pub fn routes() -> Router<AppState> {
    Router::new().route("/{id}/packs/notes/{pack}", get(read).put(write))
}

/// A long note is a document, and this is a reminder.
const MOST_TEXT: usize = 4000;
/// More buttons than this is a console, not a note.
const MOST_COMMANDS: usize = 20;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Note {
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub commands: Vec<Saved>,
}

/// One command worth a button, as the operator labelled it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Saved {
    pub label: String,
    pub command: String,
}

impl Note {
    fn empty(&self) -> bool {
        self.text.is_empty() && self.commands.is_empty()
    }
}

#[derive(FromRow)]
struct Row {
    pack_uuid: String,
    text: String,
    commands: String,
}

impl Row {
    fn note(self) -> Note {
        Note {
            text: self.text,
            commands: serde_json::from_str(&self.commands).unwrap_or_default(),
        }
    }
}

/// Every note on one server, by pack id. Ids are kept lowercase, since a
/// manifest may write one either way and the panel should not care.
pub(crate) async fn all(state: &AppState, id: Uuid) -> Result<HashMap<String, Note>, sqlx::Error> {
    let rows: Vec<Row> =
        sqlx::query_as("SELECT pack_uuid, text, commands FROM pack_notes WHERE server_id = ?")
            .bind(id.to_string())
            .fetch_all(&state.db)
            .await?;
    Ok(rows
        .into_iter()
        .map(|row| (row.pack_uuid.to_ascii_lowercase(), row.note()))
        .collect())
}

/// A pack listing with each note beside its pack, and null where there is
/// none. Shared with the MCP, which lists the same packs.
pub(crate) async fn attach(
    state: &AppState,
    id: Uuid,
    packs: Vec<crate::packs::Installed>,
) -> Result<Vec<serde_json::Value>, ApiError> {
    let notes = all(state, id).await?;
    Ok(packs
        .into_iter()
        .map(|pack| {
            let note = pack
                .uuid
                .as_deref()
                .and_then(|uuid| notes.get(&uuid.to_ascii_lowercase()));
            let mut value = serde_json::to_value(&pack).unwrap_or_default();
            if let Some(object) = value.as_object_mut() {
                object.insert("note".into(), json!(note));
            }
            value
        })
        .collect())
}

async fn one(state: &AppState, id: Uuid, pack: &str) -> Result<Note, sqlx::Error> {
    let row: Option<Row> = sqlx::query_as(
        "SELECT pack_uuid, text, commands FROM pack_notes WHERE server_id = ? AND pack_uuid = ?",
    )
    .bind(id.to_string())
    .bind(pack)
    .fetch_optional(&state.db)
    .await?;
    Ok(row.map(Row::note).unwrap_or_default())
}

/// The note, and what the pack itself looks like it answers to.
async fn read(
    identity: Identity,
    State(state): State<AppState>,
    Path((id, pack)): Path<(Uuid, Uuid)>,
) -> ApiResult<impl IntoResponse> {
    let (directory, level, bedrock) = super::packs::located(&identity, &state, id).await?;
    let key = pack.to_string();
    let note = one(&state, id, &key).await?;

    let wanted = key.clone();
    let suggestions = tokio::task::spawn_blocking(move || {
        crate::packs::installed(&directory, &level, bedrock)
            .into_iter()
            .find(|one| {
                one.uuid
                    .as_deref()
                    .is_some_and(|found| found.eq_ignore_ascii_case(&wanted))
            })
            .map(|one| {
                // A listing gives its path with forward slashes, whatever the
                // platform underneath.
                let at = one
                    .path
                    .split('/')
                    .fold(directory.clone(), |so_far, part| so_far.join(part));
                crate::packs::suggestions(&at)
            })
            .unwrap_or_default()
    })
    .await
    .map_err(|error| ApiError::Internal(error.into()))?;

    Ok(OkJson(json!({
        "text": note.text,
        "commands": note.commands,
        "suggestions": suggestions,
    })))
}

/// Saves a note, or forgets one that has been emptied.
async fn write(
    identity: Identity,
    State(state): State<AppState>,
    Path((id, pack)): Path<(Uuid, Uuid)>,
    Json(body): Json<Note>,
) -> ApiResult<impl IntoResponse> {
    // Writing one is a config change: the buttons it saves are there for
    // everybody who can reach the server, the same as a console macro.
    identity
        .require_server(&state.db, id, ServerPerm::Config)
        .await?;
    let note = checked(body)?;
    let key = pack.to_string();

    if note.empty() {
        sqlx::query("DELETE FROM pack_notes WHERE server_id = ? AND pack_uuid = ?")
            .bind(id.to_string())
            .bind(&key)
            .execute(&state.db)
            .await?;
    } else {
        sqlx::query(
            "INSERT INTO pack_notes (server_id, pack_uuid, text, commands, updated_at)
             VALUES (?, ?, ?, ?, ?)
             ON CONFLICT(server_id, pack_uuid) DO UPDATE SET
                 text = excluded.text,
                 commands = excluded.commands,
                 updated_at = excluded.updated_at",
        )
        .bind(id.to_string())
        .bind(&key)
        .bind(&note.text)
        .bind(serde_json::to_string(&note.commands).unwrap_or_else(|_| "[]".to_string()))
        .bind(Utc::now().to_rfc3339())
        .execute(&state.db)
        .await?;
    }

    super::audit(
        &state,
        Some(&identity.user),
        Some(id),
        match note.empty() {
            true => "cleared the note on an add-on",
            false => "wrote a note on an add-on",
        },
        Some(&key),
    )
    .await;
    Ok(OkJson(json!(note)))
}

/// A note worth keeping: trimmed, bounded, and every command one line the
/// console would take as typed.
fn checked(note: Note) -> Result<Note, ApiError> {
    let text = note.text.trim().to_string();
    if text.chars().count() > MOST_TEXT {
        return Err(ApiError::field("text", "That note is too long to keep."));
    }
    if note.commands.len() > MOST_COMMANDS {
        return Err(ApiError::field(
            "commands",
            "That is more buttons than a note should carry.",
        ));
    }

    let mut commands = Vec::with_capacity(note.commands.len());
    for one in note.commands {
        let (label, command) = super::macros::checked(&one.label, &one.command)?;
        // The console takes a command as the server reads it, without the
        // slash a player would type.
        let command = command.trim_start_matches('/').trim().to_string();
        if command.is_empty() {
            return Err(ApiError::field("command", "Say what it should run."));
        }
        commands.push(Saved { label, command });
    }

    Ok(Note { text, commands })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(text: &str, commands: &[(&str, &str)]) -> Note {
        Note {
            text: text.to_string(),
            commands: commands
                .iter()
                .map(|(label, command)| Saved {
                    label: (*label).to_string(),
                    command: (*command).to_string(),
                })
                .collect(),
        }
    }

    #[test]
    fn a_command_is_kept_the_way_the_console_takes_it() {
        let kept = checked(note(" read me ", &[("Reset", " /function reset ")])).expect("checked");
        assert_eq!(kept.text, "read me");
        assert_eq!(kept.commands[0].command, "function reset");
        assert_eq!(kept.commands[0].label, "Reset");
    }

    #[test]
    fn an_emptied_note_is_one_to_forget() {
        assert!(checked(note("   ", &[])).expect("checked").empty());
        assert!(!checked(note("something", &[])).expect("checked").empty());
    }

    #[test]
    fn a_command_that_is_only_a_slash_is_refused() {
        assert!(checked(note("", &[("Go", "/")])).is_err());
        assert!(checked(note("", &[("", "function x")])).is_err());
    }

    #[test]
    fn there_is_a_limit_to_both_halves() {
        let long = "x".repeat(MOST_TEXT + 1);
        assert!(checked(note(&long, &[])).is_err());
        let many: Vec<(&str, &str)> = (0..=MOST_COMMANDS).map(|_| ("Go", "say hello")).collect();
        assert!(checked(note("", &many)).is_err());
    }
}
