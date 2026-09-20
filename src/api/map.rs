use std::path::Path as FsPath;

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::{Router, routing::get};
use serde_json::json;
use uuid::Uuid;

use crate::auth::Identity;
use crate::error::{ApiError, ApiResult, Ok as OkJson};
use crate::perms::Server as ServerPerm;
use crate::state::AppState;

/// Merged into the servers router, where the id in the path comes from.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/{id}/map", get(look))
        .route("/{id}/positions", get(positions))
}

/// A web map somebody has installed on the server. The panel does not render
/// one itself; these projects already do it far better than a panel could.
struct Project {
    id: &'static str,
    name: &'static str,
    /// Where its settings live, relative to the server folder. The first that
    /// exists wins, so a plugin and a mod install both resolve.
    files: &'static [&'static str],
    /// The key that names the port, and the port it uses when nothing says.
    port_key: &'static str,
    port_default: u16,
    /// The block the port sits in, for the ones that nest it.
    section: Option<&'static str>,
}

const PROJECTS: [Project; 4] = [
    Project {
        id: "squaremap",
        name: "squaremap",
        files: &[
            "plugins/squaremap/config.yml",
            "config/squaremap/config.yml",
        ],
        port_key: "port",
        port_default: 8080,
        section: Some("internal-webserver"),
    },
    Project {
        id: "pl3xmap",
        name: "Pl3xMap",
        files: &["plugins/Pl3xMap/config.yml", "config/Pl3xMap/config.yml"],
        port_key: "port",
        port_default: 8080,
        section: Some("internal-webserver"),
    },
    Project {
        id: "bluemap",
        name: "BlueMap",
        files: &[
            "plugins/BlueMap/webserver.conf",
            "config/bluemap/webserver.conf",
        ],
        port_key: "port",
        port_default: 8100,
        section: None,
    },
    Project {
        id: "dynmap",
        name: "Dynmap",
        files: &[
            "plugins/dynmap/configuration.txt",
            "config/dynmap/configuration.txt",
        ],
        port_key: "webserver-port",
        port_default: 8123,
        section: None,
    },
];

/// Reads a port out of one of these settings files. They are all YAML or close
/// enough to it that a line scan is honest: indentation says what nests where,
/// and anything unreadable falls back to the project's own default.
fn port_in(text: &str, key: &str, section: Option<&str>) -> Option<u16> {
    let mut depth: Option<usize> = None;

    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = line.len() - trimmed.len();

        if let Some(wanted) = section {
            match depth {
                // Not in the block yet: only its own heading gets us in.
                None => {
                    if trimmed.starts_with(wanted)
                        && trimmed[wanted.len()..].trim_start().starts_with(':')
                    {
                        depth = Some(indent);
                    }
                    continue;
                }
                // Out again the moment something at the same level starts.
                Some(opened) if indent <= opened => {
                    depth = None;
                    continue;
                }
                Some(_) => {}
            }
        }

        let Some(rest) = trimmed.strip_prefix(key) else {
            continue;
        };
        let rest = rest.trim_start();
        let Some(value) = rest.strip_prefix(':').or_else(|| rest.strip_prefix('=')) else {
            continue;
        };
        if let Ok(port) = value.trim().trim_matches('"').parse::<u16>() {
            return Some(port);
        }
    }
    None
}

/// True unless the file plainly turns the web server off.
fn enabled_in(text: &str) -> bool {
    !text
        .lines()
        .map(str::trim)
        .any(|line| line == "enabled: false" || line == "enabled=false")
}

async fn look(
    identity: Identity,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<impl IntoResponse> {
    identity
        .require_server(&state.db, id, ServerPerm::Console)
        .await?;
    let row = super::servers::load(&state, id).await?;
    let directory = FsPath::new(&row.directory);

    for project in &PROJECTS {
        for relative in project.files {
            let path = directory.join(relative);
            let Ok(text) = tokio::fs::read_to_string(&path).await else {
                continue;
            };
            let port =
                port_in(&text, project.port_key, project.section).unwrap_or(project.port_default);
            return Ok(OkJson(json!({
                "found": true,
                "id": project.id,
                "name": project.name,
                "port": port,
                "enabled": enabled_in(&text),
                "answering": answering(port).await,
                "config": relative,
            })));
        }
    }

    Ok(OkJson(json!({ "found": false })))
}

/// A map only helps if something is actually listening, and a plugin that has
/// not started yet looks exactly like one that is not installed.
async fn answering(port: u16) -> bool {
    let address = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    tokio::time::timeout(
        std::time::Duration::from_millis(400),
        tokio::net::TcpStream::connect(address),
    )
    .await
    .is_ok_and(|result| result.is_ok())
}

/// Whoever is on, and where they are standing.
///
/// This asks the server itself over RCON rather than reading the world, so it
/// needs no plugin and works on vanilla. It also needs RCON: commands sent over
/// stdin come back with no answer, and there is nothing to read.
async fn positions(
    identity: Identity,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<impl IntoResponse> {
    identity
        .require_server(&state.db, id, ServerPerm::Console)
        .await?;
    let row = super::servers::load(&state, id).await?;

    if row.kind != "minecraft_java" {
        return Ok(OkJson(json!({
            "supported": false,
            "reason": "Only Java servers answer this one: Bedrock has no RCON.",
            "players": [],
        })));
    }
    if !state.supervisor.state(id).await.is_live() {
        return Ok(OkJson(json!({
            "supported": true,
            "reason": "The server is not running.",
            "players": [],
        })));
    }

    // Checked before anything is sent: without RCON the commands would go out
    // over stdin on every poll and no answer would ever come back.
    if !state.supervisor.can_ask(id).await {
        return Ok(OkJson(json!({
            "supported": false,
            "reason": "This needs RCON, so the panel can read what the server says back.",
            "players": [],
        })));
    }

    let listed = state
        .supervisor
        .ask_quietly(id, "list")
        .await
        .map_err(|error| ApiError::conflict(error.to_string()))?;
    let Some(output) = listed.output else {
        return Ok(OkJson(json!({
            "supported": false,
            "reason": "This needs RCON, so the panel can read what the server says back.",
            "players": [],
        })));
    };

    let mut found = Vec::new();
    // A round trip each, so a busy server is not held up reading the whole lobby.
    for name in names_in(&output).into_iter().take(40) {
        let spot = state
            .supervisor
            .ask_quietly(id, &format!("data get entity {name} Pos"))
            .await
            .ok()
            .and_then(|answer| answer.output)
            .as_deref()
            .and_then(place_in);
        let Some((x, y, z)) = spot else { continue };
        let dimension = state
            .supervisor
            .ask_quietly(id, &format!("data get entity {name} Dimension"))
            .await
            .ok()
            .and_then(|answer| answer.output)
            .as_deref()
            .and_then(dimension_in)
            .unwrap_or_else(|| "minecraft:overworld".into());
        found.push(json!({ "name": name, "x": x, "y": y, "z": z, "dimension": dimension }));
    }

    Ok(OkJson(json!({
        "supported": true,
        "players": found,
    })))
}

/// Names out of `/list`, which both the modern and the older wording end with
/// a colon and a comma-separated tail.
fn names_in(output: &str) -> Vec<String> {
    let Some((_, tail)) = output.split_once(':') else {
        return Vec::new();
    };
    tail.split(',')
        .map(str::trim)
        // Some forks decorate a name with its world in brackets.
        .map(|name| name.split_whitespace().next().unwrap_or(name))
        .filter(|name| {
            !name.is_empty()
                && name
                    .chars()
                    .all(|c| c.is_alphanumeric() || c == '_' || c == '.')
        })
        .map(str::to_string)
        .collect()
}

/// `Notch has the following entity data: [1.5d, 64.0d, -3.5d]`
fn place_in(output: &str) -> Option<(f64, f64, f64)> {
    let inside = output.split_once('[')?.1.split_once(']')?.0;
    let mut parts = inside
        .split(',')
        .map(|one| one.trim().trim_end_matches(['d', 'f']).parse::<f64>());
    let x = parts.next()?.ok()?;
    let y = parts.next()?.ok()?;
    let z = parts.next()?.ok()?;
    Some((x, y, z))
}

/// `Notch has the following entity data: "minecraft:overworld"`
fn dimension_in(output: &str) -> Option<String> {
    let (_, tail) = output.rsplit_once(": ")?;
    let name = tail.trim().trim_matches('"');
    (!name.is_empty()).then(|| name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_nested_port_and_ignores_one_in_another_block() {
        let text = "settings:\n  web-address: ''\n  other:\n    port: 1111\n  internal-webserver:\n    enabled: true\n    bind: 0.0.0.0\n    port: 8123\n";
        assert_eq!(
            port_in(text, "port", Some("internal-webserver")),
            Some(8123)
        );
    }

    #[test]
    fn reads_a_flat_port() {
        let text = "# BlueMap\nenabled: true\nwebroot: \"bluemap/web\"\nport: 8100\n";
        assert_eq!(port_in(text, "port", None), Some(8100));
        assert!(enabled_in(text));
    }

    #[test]
    fn reads_dynmaps_own_spelling() {
        let text = "webserver-bindaddress: 0.0.0.0\nwebserver-port: 8123\n";
        assert_eq!(port_in(text, "webserver-port", None), Some(8123));
    }

    #[test]
    fn a_turned_off_web_server_says_so() {
        assert!(!enabled_in("enabled: false\nport: 8100\n"));
    }

    #[test]
    fn a_missing_section_finds_nothing_rather_than_the_wrong_port() {
        let text = "settings:\n  other:\n    port: 1111\n";
        assert_eq!(port_in(text, "port", Some("internal-webserver")), None);
    }

    #[test]
    fn reads_the_names_out_of_list() {
        let output = "There are 2 of a max of 20 players online: Notch, jeb_";
        assert_eq!(names_in(output), vec!["Notch", "jeb_"]);
    }

    #[test]
    fn reads_the_names_out_of_the_older_wording() {
        let output = "There are 1/20 players online:\nSteve";
        assert_eq!(names_in(output), vec!["Steve"]);
    }

    #[test]
    fn an_empty_server_lists_nobody() {
        assert!(names_in("There are 0 of a max of 20 players online: ").is_empty());
    }

    #[test]
    fn reads_a_position() {
        let output = "Notch has the following entity data: [1.5d, 64.0d, -3.5d]";
        assert_eq!(place_in(output), Some((1.5, 64.0, -3.5)));
    }

    #[test]
    fn reads_a_dimension() {
        let output = "Notch has the following entity data: \"minecraft:the_nether\"";
        assert_eq!(
            dimension_in(output).as_deref(),
            Some("minecraft:the_nether")
        );
    }
}
