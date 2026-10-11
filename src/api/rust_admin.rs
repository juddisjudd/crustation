use std::collections::{BTreeMap, HashMap};

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::{Json, Router, routing::get, routing::put};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::auth::Identity;
use crate::error::{ApiError, ApiResult, Ok as OkJson};
use crate::perms::Server as ServerPerm;
use crate::rust_reference::{self, Listed};
use crate::state::AppState;

use super::servers::ServerRow;

const MASK: &str = "********";

/// Merged into the servers router, where the id in the path comes from.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/{id}/rust/convars", get(convars))
        .route("/{id}/rust/convars/{name}", put(set_convar))
        .route("/{id}/rust/commands", get(commands))
}

async fn load_rust(state: &AppState, id: Uuid) -> Result<ServerRow, ApiError> {
    let row = super::servers::load(state, id).await?;
    if row.kind != crate::rust_game::KIND {
        return Err(ApiError::validation("Only Rust servers answer here."));
    }
    Ok(row)
}

/// The running server's own list, or `None` when it is down or did not answer.
async fn listed(state: &AppState, id: Uuid) -> Option<Listed> {
    // `find .` reads every convar, and one of them throws into the console until Steam is up.
    if state.supervisor.state(id).await != crate::supervisor::State::Running {
        return None;
    }
    let text = state.supervisor.ask(id, "find .").await.ok()?;
    let listed = rust_reference::parse_find(&text);
    (!listed.convars.is_empty()).then_some(listed)
}

async fn convars(
    identity: Identity,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<impl IntoResponse> {
    let granted = identity
        .require_server(&state.db, id, ServerPerm::Commands)
        .await?;
    let row = load_rust(&state, id).await?;
    let running = state.supervisor.state(id).await.is_live();
    let live = match running {
        true => listed(&state, id).await,
        false => None,
    };
    let catalogue = rust_reference::convars(&state.http).await;
    if live.is_none()
        && let Err(error) = &catalogue
    {
        return Err(ApiError::Unavailable(error.clone()));
    }

    let known: HashMap<&str, &rust_reference::Convar> = catalogue
        .as_deref()
        .map(|list| list.iter().map(|one| (one.name.as_str(), one)).collect())
        .unwrap_or_default();
    let startup: BTreeMap<String, String> = crate::rust_game::visible_convars(&row.command)
        .into_iter()
        .collect();

    // A running server's list is the build it runs, so it decides which names exist.
    let mut rows: BTreeMap<String, Value> = BTreeMap::new();
    match &live {
        Some(live) => {
            for (name, (help, value)) in &live.convars {
                let entry = known.get(name.as_str());
                let (help, generated) = match rust_reference::clean_help(help) {
                    (help, generated) if !help.is_empty() => (help, generated),
                    _ => entry.map_or((String::new(), false), |one| {
                        (one.help.clone(), one.generated)
                    }),
                };
                let value = match crate::rust_game::is_secret(name) {
                    true => MASK.to_string(),
                    false => value.clone(),
                };
                rows.insert(
                    name.clone(),
                    json!({
                        "name": name,
                        "help": help,
                        "generated": generated,
                        "type": entry.map_or_else(|| rust_reference::guess_kind(&value), |one| one.kind),
                        "default": entry.and_then(|one| one.default.clone()),
                        "value": value,
                    }),
                );
            }
        }
        None => {
            for one in known.values() {
                rows.insert(
                    one.name.clone(),
                    json!({
                        "name": one.name,
                        "help": one.help,
                        "generated": one.generated,
                        "type": one.kind,
                        "default": one.default,
                        "value": null,
                    }),
                );
            }
        }
    }
    for (name, value) in &startup {
        let entry = rows.entry(name.clone()).or_insert_with(|| {
            json!({
                "name": name,
                "help": "",
                "generated": false,
                "type": rust_reference::guess_kind(value),
                "default": null,
                "value": null,
            })
        });
        entry["startup"] = json!(value);
    }
    for (name, entry) in rows.iter_mut() {
        entry["managed"] = json!(crate::rust_game::is_managed(name));
        if entry.get("startup").is_none() {
            entry["startup"] = Value::Null;
        }
    }

    Ok(OkJson(json!({
        "running": running,
        "live": live.is_some(),
        "can_keep": granted.contains(&ServerPerm::Config),
        "convars": rows.into_values().collect::<Vec<_>>(),
    })))
}

#[derive(Deserialize)]
struct SetConvar {
    /// `None` with `startup` takes it off the start command.
    value: Option<String>,
    #[serde(default)]
    now: bool,
    #[serde(default)]
    startup: bool,
}

async fn set_convar(
    identity: Identity,
    State(state): State<AppState>,
    Path((id, name)): Path<(Uuid, String)>,
    Json(body): Json<SetConvar>,
) -> ApiResult<impl IntoResponse> {
    identity
        .require_server(&state.db, id, ServerPerm::Commands)
        .await?;
    let row = load_rust(&state, id).await?;
    let field = format!("settings.{name}");
    if !body.now && !body.startup {
        return Err(ApiError::validation(
            "Say whether to set it now, keep it for the next start, or both.",
        ));
    }
    if crate::rust_game::is_managed(&name) {
        return Err(ApiError::field(
            field,
            "The panel keeps this one in step with the server, so it cannot be set here.",
        ));
    }
    let value = match &body.value {
        Some(value) => Some(
            crate::rust_game::check_other(&name, value)
                .map_err(|message| ApiError::field(field.clone(), message))?,
        ),
        None => None,
    };

    let running = state.supervisor.state(id).await.is_live();
    let mut live = None;
    if body.now {
        let Some(value) = &value else {
            return Err(ApiError::field(field, "Give it a value to set."));
        };
        if !running {
            return Err(ApiError::conflict(
                "The server has to be running to set it now.",
            ));
        }
        // Rust ends a quoted argument at the next quote and has no way to escape one.
        if value.contains('"') {
            return Err(ApiError::field(field, "Take out the double quotes."));
        }
        let reply = state
            .supervisor
            .ask(id, &format!("{name} \"{value}\""))
            .await
            .map_err(|error| {
                ApiError::Unavailable(format!("The server did not answer: {error}"))
            })?;
        live = Some(rust_reference::parse_reply(&name, &reply).ok_or_else(|| {
            ApiError::field(
                field.clone(),
                format!("Rust did not take it: {}", reply.trim()),
            )
        })?);
        super::audit(
            &state,
            Some(&identity.user),
            Some(id),
            "set a convar",
            Some(&format!("{name} {value}")),
        )
        .await;
    }

    if body.startup {
        identity
            .require_server(&state.db, id, ServerPerm::Config)
            .await?;
        let requested = BTreeMap::from([(
            name.clone(),
            value.clone().map_or(Value::Null, Value::String),
        )]);
        super::properties::write_rust(
            &state,
            &identity,
            &row,
            requested,
            "changed start-up settings",
        )
        .await?;
    }

    let row = load_rust(&state, id).await?;
    let startup = crate::rust_game::visible_convars(&row.command)
        .into_iter()
        .find(|(key, _)| *key == name)
        .map(|(_, value)| value);
    Ok(OkJson(json!({
        "live": live,
        "startup": startup,
        // Rust reads its command line once, so only a change it was also told now is in effect.
        "restart_required": body.startup && running && !body.now,
    })))
}

async fn commands(
    identity: Identity,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<impl IntoResponse> {
    identity
        .require_server(&state.db, id, ServerPerm::Commands)
        .await?;
    load_rust(&state, id).await?;
    let running = state.supervisor.state(id).await.is_live();
    let live = match running {
        true => listed(&state, id).await,
        false => None,
    };
    let catalogue = rust_reference::commands(&state.http).await;
    if live.is_none()
        && let Err(error) = &catalogue
    {
        return Err(ApiError::Unavailable(error.clone()));
    }

    let mut rows: BTreeMap<String, (String, bool)> = catalogue
        .as_deref()
        .map(|list| {
            list.iter()
                .map(|one| (one.name.clone(), (one.help.clone(), one.generated)))
                .collect()
        })
        .unwrap_or_default();
    if let Some(live) = &live {
        rows.retain(|name, _| live.commands.contains_key(name));
        for (name, help) in &live.commands {
            let (help, generated) = rust_reference::clean_help(help);
            let row = rows.entry(name.clone()).or_default();
            if !help.is_empty() {
                *row = (help, generated);
            }
        }
    }

    let commands: Vec<Value> = rows
        .into_iter()
        .map(|(name, (help, generated))| json!({ "name": name, "help": help, "generated": generated }))
        .collect();
    Ok(OkJson(json!({
        "running": running,
        "live": live.is_some(),
        "commands": commands,
    })))
}
