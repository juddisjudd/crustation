//! The panel as a Model Context Protocol server.
//!
//! Streamable HTTP at `/mcp`, so an assistant can read a server's console and
//! act on it rather than be told about it second hand. Every call runs as the
//! API key that carried it and goes through the same permission checks the REST
//! API uses, so an MCP client can never reach further than its key already
//! could.

use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;
use axum::extract::State;
use axum::http::request::Parts;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use rmcp::handler::server::common::Extension;
use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::model::{
    Implementation, ListResourceTemplatesResult, ListResourcesResult, PaginatedRequestParams,
    ProtocolVersion, ReadResourceRequestParams, ReadResourceResponse, ReadResourceResult, Resource,
    ResourceContents, ResourceTemplate, ServerCapabilities, ServerConfig,
};
use rmcp::service::RequestContext;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::tower::{
    StreamableHttpServerConfig, StreamableHttpService,
};
use rmcp::{ErrorData, RoleServer, ServerHandler, tool, tool_handler, tool_router};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::api::servers::ServerRow;
use crate::auth::Identity;
use crate::error::ApiError;
use crate::perms::Server as ServerPerm;
use crate::state::AppState;

/// Mounted at `/mcp`. The handler is built per session and carries the panel
/// state; who is calling arrives per request, in the HTTP parts.
pub fn service(state: AppState) -> Router<AppState> {
    let panel = state.clone();
    let mut config = StreamableHttpServerConfig::default();
    // Every call carries a bearer token and the endpoint refuses cookies, so a
    // page that rebound our hostname would still arrive with no credentials.
    // What the panel answers to is the operator's to choose, not ours.
    config.allowed_hosts = Vec::new();

    // Sessions are kept because a client's first request is a handshake that has
    // not agreed a protocol version yet, and the SDK reads one older than the
    // revision that dropped sessions. A client on the newer revision is served
    // statelessly anyway, whatever is configured here.
    let mcp = StreamableHttpService::new(
        move || Ok(Crustation::new(panel.clone())),
        Arc::new(LocalSessionManager::default()),
        config,
    );

    Router::new()
        .fallback_service(mcp)
        .layer(axum::middleware::from_fn_with_state(state, authenticate))
}

/// Turns the API key into an identity before the transport sees the request, so
/// a tool can read who is calling out of the HTTP parts.
///
/// Sessions are deliberately not accepted: a browser sends its cookie to any
/// origin that asks, and this endpoint is reachable cross-site.
async fn authenticate(
    State(state): State<AppState>,
    mut request: axum::extract::Request,
    next: Next,
) -> Response {
    if !state.mcp_on() {
        return (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            "The MCP endpoint is switched off for this panel.",
        )
            .into_response();
    }

    let token = request
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(|value| value.trim().to_string());

    let Some(token) = token.filter(|token| !token.is_empty()) else {
        return unauthenticated(
            "An API key is required. Send it as `Authorization: Bearer <key>`.",
        );
    };

    match crate::auth::identity_from_api_key(&state, &token).await {
        Ok(identity) => {
            request.extensions_mut().insert(identity);
            next.run(request).await
        }
        Err(_) => unauthenticated("That API key was not recognised."),
    }
}

fn unauthenticated(message: &'static str) -> Response {
    (
        axum::http::StatusCode::UNAUTHORIZED,
        [(
            axum::http::header::WWW_AUTHENTICATE,
            "Bearer realm=\"crustation\"",
        )],
        message,
    )
        .into_response()
}

#[derive(Clone)]
pub struct Crustation {
    state: AppState,
}

impl Crustation {
    fn new(state: AppState) -> Self {
        Self { state }
    }

    /// Who is calling. The transport puts the HTTP parts in the request
    /// context; the middleware above put the identity in the parts.
    fn caller(parts: &Parts) -> Result<&Identity, ErrorData> {
        parts.extensions.get::<Identity>().ok_or_else(|| {
            ErrorData::invalid_request("This call arrived without an API key.", None)
        })
    }

    /// Finds one server by id or by name, and checks the caller may do the
    /// thing they are asking for. A name is what an assistant has to hand, so
    /// both are accepted.
    async fn find(
        &self,
        who: &Identity,
        wanted: &str,
        need: ServerPerm,
    ) -> Result<ServerRow, ErrorData> {
        let wanted = wanted.trim();
        let row = match Uuid::parse_str(wanted) {
            Ok(id) => sqlx::query_as::<_, ServerRow>("SELECT * FROM servers WHERE id = ?")
                .bind(id.to_string())
                .fetch_optional(&self.state.db)
                .await
                .map_err(internal)?,
            Err(_) => sqlx::query_as::<_, ServerRow>(
                "SELECT * FROM servers WHERE name = ? COLLATE NOCASE",
            )
            .bind(wanted)
            .fetch_optional(&self.state.db)
            .await
            .map_err(internal)?,
        };

        let row = row.ok_or_else(|| {
            ErrorData::invalid_params(
                format!("No server called '{wanted}'. Call list_servers to see what there is."),
                None,
            )
        })?;

        who.require_server(&self.state.db, row.uuid(), need)
            .await
            .map_err(refused)?;
        Ok(row)
    }

    /// One server as the tools and resources describe it.
    async fn summarise(&self, row: &ServerRow) -> Summary {
        let id = row.uuid();
        let runtime = self.state.supervisor.runtime(id).await;
        let live = self.state.statuses.get(id).await;
        Summary {
            id: row.id.clone(),
            name: row.name.clone(),
            kind: row.kind.clone(),
            state: runtime.state_str,
            address: format!("{}:{}", row.host, row.port),
            version: live.as_ref().map(|status| status.version.clone()),
            motd: live.as_ref().map(|status| status.motd.clone()),
            players_online: live.as_ref().map(|status| status.players_online),
            players_max: live.as_ref().map(|status| status.players_max),
            started_at: runtime.started_at.map(|at| at.to_rfc3339()),
            installing: runtime.installing,
        }
    }

    /// Every server this caller can see, newest name order.
    async fn visible(&self, who: &Identity) -> Result<Vec<ServerRow>, ErrorData> {
        let rows: Vec<ServerRow> =
            sqlx::query_as("SELECT * FROM servers ORDER BY name COLLATE NOCASE")
                .fetch_all(&self.state.db)
                .await
                .map_err(internal)?;
        let allowed = who
            .visible_servers(&self.state.db)
            .await
            .map_err(internal)?;
        Ok(match allowed {
            None => rows,
            Some(ids) => rows
                .into_iter()
                .filter(|row| ids.contains(&row.id))
                .collect(),
        })
    }
}

fn internal(error: impl std::fmt::Display) -> ErrorData {
    tracing::warn!(%error, "an MCP call failed");
    ErrorData::internal_error("The panel could not answer that.", None)
}

/// A refusal the caller can act on: which permission, on which server.
fn refused(error: ApiError) -> ErrorData {
    match error {
        ApiError::Internal(error) => internal(error),
        other => ErrorData::invalid_request(other.to_string(), None),
    }
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct Summary {
    id: String,
    name: String,
    /// `minecraft_java` or `minecraft_bedrock`.
    kind: String,
    /// `stopped`, `starting`, `running`, `stopping` or `crashed`.
    state: String,
    address: String,
    /// What the server said about itself on the last ping; absent when it is down.
    version: Option<String>,
    motd: Option<String>,
    players_online: Option<i64>,
    players_max: Option<i64>,
    started_at: Option<String>,
    installing: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct Which {
    /// The server's id, or its name.
    server: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct Act {
    /// The server's id, or its name.
    server: String,
    /// `start`, `stop`, `restart` or `kill`. `kill` ends the process without
    /// letting the game save, so keep it for one that has stopped answering.
    action: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct Command {
    /// The server's id, or its name.
    server: String,
    /// The command as you would type it in the console, with or without a
    /// leading slash.
    command: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct Tail {
    /// The server's id, or its name.
    server: String,
    /// How many lines from the end. 100 by default, 500 at most.
    lines: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct Browse {
    /// The server's id, or its name.
    server: String,
    /// A folder inside the server, relative to its own root. Empty for the top.
    path: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadFile {
    /// The server's id, or its name.
    server: String,
    /// The file, relative to the server's own root.
    path: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct WriteFile {
    /// The server's id, or its name.
    server: String,
    /// The file, relative to the server's own root. Folders along the way that
    /// do not exist yet are made.
    path: String,
    /// The whole new contents. The file is replaced, not added to.
    content: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SetProperty {
    /// The server's id, or its name.
    server: String,
    /// A key from `server.properties`, for example `difficulty`.
    key: String,
    /// The new value, as it would be written in the file.
    value: String,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct Ran {
    /// `rcon` when the panel asked over RCON, `stdin` when it typed it in.
    via: String,
    /// What the server replied. RCON answers; standard input does not, so this
    /// is empty there and the reply turns up in the console instead.
    output: String,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct Line {
    at: String,
    stream: String,
    text: String,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct Entry {
    name: String,
    /// `file` or `directory`.
    kind: String,
    size: u64,
    modified: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct Overview {
    version: String,
    started_at: String,
    servers: usize,
    running: usize,
    host_cpu_percent: f64,
    host_memory_used_bytes: u64,
    host_memory_total_bytes: u64,
}

#[tool_router]
impl Crustation {
    /// Every game server this key can see, with what each is doing right now.
    /// Start here: the ids it returns are what the other tools take.
    #[tool]
    async fn list_servers(
        &self,
        Extension(parts): Extension<Parts>,
    ) -> Result<Json<Vec<Summary>>, ErrorData> {
        let who = Self::caller(&parts)?;
        let rows = self.visible(who).await?;
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            out.push(self.summarise(&row).await);
        }
        Ok(Json(out))
    }

    /// One server in full: what it is, where it listens, and what it last said
    /// about itself over the wire.
    #[tool]
    async fn server_details(
        &self,
        Extension(parts): Extension<Parts>,
        Parameters(Which { server }): Parameters<Which>,
    ) -> Result<Json<Summary>, ErrorData> {
        let who = Self::caller(&parts)?;
        let row = self.find(who, &server, ServerPerm::Logs).await?;
        Ok(Json(self.summarise(&row).await))
    }

    /// Starts, stops, restarts or kills a server. Stopping asks the game to save
    /// and shut down; killing does not wait. Needs the COMMANDS permission.
    #[tool]
    async fn server_action(
        &self,
        Extension(parts): Extension<Parts>,
        Parameters(Act { server, action }): Parameters<Act>,
    ) -> Result<Json<Summary>, ErrorData> {
        let who = Self::caller(&parts)?;
        let row = self.find(who, &server, ServerPerm::Commands).await?;
        let id = row.uuid();

        let supervisor = self.state.supervisor.clone();
        let done = match action.trim().to_ascii_lowercase().as_str() {
            "start" => supervisor.start_by_id(id).await,
            "stop" => supervisor.stop(id).await,
            "restart" => supervisor.restart(id).await,
            "kill" => supervisor.kill(id).await,
            other => {
                return Err(ErrorData::invalid_params(
                    format!("'{other}' is not an action. Use start, stop, restart or kill."),
                    None,
                ));
            }
        };
        done.map_err(|error| ErrorData::invalid_request(error.to_string(), None))?;

        crate::api::audit(
            &self.state,
            Some(&who.user),
            Some(id),
            &format!("sent {action} to a server over MCP"),
            Some(&row.name),
        )
        .await;
        Ok(Json(self.summarise(&row).await))
    }

    /// Runs one console command on a running server. On Java the panel uses RCON
    /// where it is set up and returns what the server said; on Bedrock, and on a
    /// Java server without RCON, it types the command in and the reply turns up
    /// in the console. Needs the COMMANDS permission.
    #[tool]
    async fn send_command(
        &self,
        Extension(parts): Extension<Parts>,
        Parameters(Command { server, command }): Parameters<Command>,
    ) -> Result<Json<Ran>, ErrorData> {
        let who = Self::caller(&parts)?;
        let row = self.find(who, &server, ServerPerm::Commands).await?;
        let command = command.trim();
        if command.is_empty() {
            return Err(ErrorData::invalid_params(
                "There is no command there.",
                None,
            ));
        }

        let outcome = self
            .state
            .supervisor
            .run_command(row.uuid(), command)
            .await
            .map_err(|error| ErrorData::invalid_request(error.to_string(), None))?;

        crate::api::audit(
            &self.state,
            Some(&who.user),
            Some(row.uuid()),
            "ran a console command over MCP",
            Some(command),
        )
        .await;
        Ok(Json(Ran {
            via: outcome.via.to_string(),
            output: outcome.output.unwrap_or_default(),
        }))
    }

    /// The tail of a server's console, oldest line first. This is where a crash,
    /// a stack trace or a plugin complaining about itself will be. Needs the
    /// CONSOLE permission.
    #[tool]
    async fn read_console(
        &self,
        Extension(parts): Extension<Parts>,
        Parameters(Tail { server, lines }): Parameters<Tail>,
    ) -> Result<Json<Vec<Line>>, ErrorData> {
        let who = Self::caller(&parts)?;
        let row = self.find(who, &server, ServerPerm::Console).await?;
        Ok(Json(self.tail(row.uuid(), lines).await))
    }

    /// Who is on a server now, as the server itself named them on the last ping.
    /// Bedrock's ping carries no names, so it reports the count alone. Needs the
    /// PLAYERS permission.
    #[tool]
    async fn list_players(
        &self,
        Extension(parts): Extension<Parts>,
        Parameters(Which { server }): Parameters<Which>,
    ) -> Result<Json<serde_json::Value>, ErrorData> {
        let who = Self::caller(&parts)?;
        let row = self.find(who, &server, ServerPerm::Players).await?;
        let live = self.state.statuses.get(row.uuid()).await;
        Ok(Json(serde_json::json!({
            "online": live.as_ref().map(|status| status.players_online).unwrap_or(0),
            "max": live.as_ref().map(|status| status.players_max).unwrap_or(0),
            "named": live.map(|status| status.sample).unwrap_or_default(),
        })))
    }

    /// Everything in a server's `server.properties`. Needs the CONFIG permission.
    #[tool]
    async fn read_properties(
        &self,
        Extension(parts): Extension<Parts>,
        Parameters(Which { server }): Parameters<Which>,
    ) -> Result<Json<std::collections::BTreeMap<String, String>>, ErrorData> {
        let who = Self::caller(&parts)?;
        let row = self.find(who, &server, ServerPerm::Config).await?;
        Ok(Json(self.properties_of(&row).await?))
    }

    /// Changes one key in `server.properties`. The value is checked against the
    /// catalogue for that edition first, and the keys the panel writes itself
    /// are refused. Minecraft reads the file once, at startup, so the server has
    /// to be restarted for it to take. Needs the CONFIG permission.
    #[tool]
    async fn set_property(
        &self,
        Extension(parts): Extension<Parts>,
        Parameters(SetProperty { server, key, value }): Parameters<SetProperty>,
    ) -> Result<Json<serde_json::Value>, ErrorData> {
        let who = Self::caller(&parts)?;
        let row = self.find(who, &server, ServerPerm::Config).await?;

        if crate::properties::is_reserved(&key) {
            return Err(ErrorData::invalid_params(
                format!("The panel writes '{key}' itself; change it on the server instead."),
                None,
            ));
        }
        let checked = match crate::properties::known(&row.kind, &key) {
            Some(entry) => crate::properties::check(entry, &value),
            None => crate::properties::check_raw(&key, &value),
        }
        .map_err(|message| ErrorData::invalid_params(message, None))?;

        let path = crate::properties::path_in(&PathBuf::from(&row.directory));
        let mut file = crate::properties::Properties::load_if_present(&path)
            .await
            .map_err(internal)?
            .unwrap_or_else(|| crate::properties::Properties::empty(&path));
        file.set(&key, &checked);
        file.save().await.map_err(internal)?;

        crate::api::audit(
            &self.state,
            Some(&who.user),
            Some(row.uuid()),
            "changed server.properties over MCP",
            Some(&key),
        )
        .await;
        Ok(Json(serde_json::json!({
            "key": key,
            "value": checked,
            "restart_required": self.state.supervisor.state(row.uuid()).await.is_live(),
        })))
    }

    /// What is in one folder of a server, directories first. Paths are relative
    /// to the server's own root and cannot reach outside it. Needs the FILES
    /// permission.
    #[tool]
    async fn list_files(
        &self,
        Extension(parts): Extension<Parts>,
        Parameters(Browse { server, path }): Parameters<Browse>,
    ) -> Result<Json<Vec<Entry>>, ErrorData> {
        let who = Self::caller(&parts)?;
        let row = self.find(who, &server, ServerPerm::Files).await?;
        let root = PathBuf::from(&row.directory);
        let at = crate::files::resolve(&root, path.as_deref().unwrap_or(""))
            .map_err(|refused| ErrorData::invalid_params(refused.to_string(), None))?;

        let found = crate::files::list(&at).await.map_err(internal)?;
        Ok(Json(
            found
                .into_iter()
                .map(|entry| Entry {
                    name: entry.name,
                    kind: entry.kind.to_string(),
                    size: entry.size,
                    modified: entry.modified,
                })
                .collect(),
        ))
    }

    /// Reads a text file out of a server's folder: a config, a log, a crash
    /// report. Anything binary or over 256 KB is refused rather than guessed at.
    /// Needs the FILES permission.
    #[tool]
    async fn read_file(
        &self,
        Extension(parts): Extension<Parts>,
        Parameters(ReadFile { server, path }): Parameters<ReadFile>,
    ) -> Result<Json<serde_json::Value>, ErrorData> {
        const LIMIT: u64 = 256 * 1024;

        let who = Self::caller(&parts)?;
        let row = self.find(who, &server, ServerPerm::Files).await?;
        let root = PathBuf::from(&row.directory);
        let at = crate::files::resolve(&root, &path)
            .map_err(|refused| ErrorData::invalid_params(refused.to_string(), None))?;

        let size = tokio::fs::metadata(&at)
            .await
            .map_err(|_| ErrorData::invalid_params(format!("There is no {path}."), None))?
            .len();
        if size > LIMIT {
            return Err(ErrorData::invalid_params(
                format!("{path} is {size} bytes, which is more than this tool will read."),
                None,
            ));
        }

        let bytes = tokio::fs::read(&at).await.map_err(internal)?;
        if crate::files::looks_binary(&bytes) {
            return Err(ErrorData::invalid_params(
                format!("{path} is not text."),
                None,
            ));
        }
        Ok(Json(serde_json::json!({
            "path": path,
            "content": String::from_utf8_lossy(&bytes),
        })))
    }

    /// Writes a text file into a server's folder, replacing whatever was there
    /// and making the folders along the path if they are missing. Refuses a
    /// folder, and refuses more than the 256 KB `read_file` will read back, so
    /// nothing is written that cannot be read again. Needs the FILES permission.
    #[tool]
    async fn write_file(
        &self,
        Extension(parts): Extension<Parts>,
        Parameters(WriteFile {
            server,
            path,
            content,
        }): Parameters<WriteFile>,
    ) -> Result<Json<serde_json::Value>, ErrorData> {
        const LIMIT: usize = 256 * 1024;

        let who = Self::caller(&parts)?;
        let row = self.find(who, &server, ServerPerm::Files).await?;
        let root = PathBuf::from(&row.directory);
        let at = crate::files::resolve(&root, &path)
            .map_err(|refused| ErrorData::invalid_params(refused.to_string(), None))?;

        if content.len() > LIMIT {
            return Err(ErrorData::invalid_params(
                format!(
                    "{path} would be {} bytes, which is more than this tool will write.",
                    content.len()
                ),
                None,
            ));
        }

        let existing = tokio::fs::metadata(&at).await.ok();
        if existing.as_ref().is_some_and(std::fs::Metadata::is_dir) {
            return Err(ErrorData::invalid_params(
                format!("{path} is a folder, not a file."),
                None,
            ));
        }

        if let Some(parent) = at.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(internal)?;
        }
        tokio::fs::write(&at, content.as_bytes())
            .await
            .map_err(internal)?;

        crate::api::audit(
            &self.state,
            Some(&who.user),
            Some(row.uuid()),
            "wrote a file over MCP",
            Some(&path),
        )
        .await;
        Ok(Json(serde_json::json!({
            "path": path,
            "bytes": content.len(),
            "created": existing.is_none(),
        })))
    }

    /// The add-ons installed on a server, read off the folders rather than out
    /// of a register, so one dropped in by hand is listed too, with whatever
    /// note has been written about each. Needs the FILES permission.
    #[tool]
    async fn list_packs(
        &self,
        Extension(parts): Extension<Parts>,
        Parameters(Which { server }): Parameters<Which>,
    ) -> Result<Json<serde_json::Value>, ErrorData> {
        let who = Self::caller(&parts)?;
        let row = self.find(who, &server, ServerPerm::Files).await?;
        let bedrock = row.kind == "minecraft_bedrock";
        let level = crate::api::packs::level_name(&row, bedrock).await;
        let directory = PathBuf::from(&row.directory);

        let found = tokio::task::spawn_blocking(move || {
            crate::packs::installed(&directory, &level, bedrock)
        })
        .await
        .map_err(internal)?;
        let packs = crate::api::pack_notes::attach(&self.state, row.uuid(), found)
            .await
            .map_err(internal)?;
        Ok(Json(serde_json::json!({ "packs": packs })))
    }

    /// The panel itself: how long it has been up, how many servers it holds, and
    /// what the machine underneath is doing.
    #[tool]
    async fn panel_overview(
        &self,
        Extension(parts): Extension<Parts>,
    ) -> Result<Json<Overview>, ErrorData> {
        let who = Self::caller(&parts)?;
        let rows = self.visible(who).await?;
        let running = self.state.supervisor.running_ids().await;

        let mut system = sysinfo::System::new();
        system.refresh_memory();
        system.refresh_cpu_usage();
        // One sample says nothing about CPU; take a second after a short pause.
        tokio::time::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL).await;
        system.refresh_cpu_usage();
        let total = system.total_memory();

        Ok(Json(Overview {
            version: env!("CARGO_PKG_VERSION").to_string(),
            started_at: self.state.started_at.to_rfc3339(),
            servers: rows.len(),
            running: rows
                .iter()
                .filter(|row| running.contains(&row.uuid()))
                .count(),
            host_cpu_percent: system.global_cpu_usage() as f64,
            host_memory_used_bytes: total.saturating_sub(system.available_memory()),
            host_memory_total_bytes: total,
        }))
    }
}

/// Work shared between the tools and the resources, so a resource read answers
/// exactly what the matching tool would.
impl Crustation {
    async fn tail(&self, id: Uuid, lines: Option<usize>) -> Vec<Line> {
        let wanted = lines.unwrap_or(100).clamp(1, 500);
        let all = self.state.supervisor.console(id, None).await;
        all.into_iter()
            .rev()
            .take(wanted)
            .rev()
            .map(|line| Line {
                at: line.at.to_rfc3339(),
                stream: line.stream.to_string(),
                text: line.text,
            })
            .collect()
    }

    async fn properties_of(
        &self,
        row: &ServerRow,
    ) -> Result<std::collections::BTreeMap<String, String>, ErrorData> {
        let path = crate::properties::path_in(&PathBuf::from(&row.directory));
        Ok(crate::properties::Properties::load_if_present(&path)
            .await
            .map_err(internal)?
            .map(|file| file.entries().into_iter().collect())
            .unwrap_or_default())
    }
}

/// What each tool needs before it will answer, for the panel screen to show.
/// The names are checked against the router itself in the tests below, so a tool
/// cannot be added without saying what it costs.
pub fn catalogue() -> Vec<(&'static str, Option<ServerPerm>)> {
    vec![
        ("list_servers", None),
        ("server_details", Some(ServerPerm::Logs)),
        ("server_action", Some(ServerPerm::Commands)),
        ("send_command", Some(ServerPerm::Commands)),
        ("read_console", Some(ServerPerm::Console)),
        ("list_players", Some(ServerPerm::Players)),
        ("read_properties", Some(ServerPerm::Config)),
        ("set_property", Some(ServerPerm::Config)),
        ("list_files", Some(ServerPerm::Files)),
        ("read_file", Some(ServerPerm::Files)),
        ("write_file", Some(ServerPerm::Files)),
        ("list_packs", Some(ServerPerm::Files)),
        ("panel_overview", None),
    ]
}

/// The tools as the panel screen lists them: the name and description the
/// router itself reports, beside the permission the call will ask for.
pub fn described() -> Vec<serde_json::Value> {
    let needs: std::collections::BTreeMap<&str, Option<ServerPerm>> =
        catalogue().into_iter().collect();

    let mut tools: Vec<_> = Crustation::tool_router()
        .list_all()
        .into_iter()
        .map(|tool| {
            serde_json::json!({
                "name": tool.name,
                "description": tool.description,
                "permission": needs
                    .get(tool.name.as_ref())
                    .and_then(|found| found.map(|one| one.as_str())),
            })
        })
        .collect();
    tools.sort_by(|left, right| left["name"].as_str().cmp(&right["name"].as_str()));
    tools
}

/// `crustation://servers/<id>/<what>`, or `crustation://servers` for the list.
fn parse_uri(uri: &str) -> Option<(String, &str)> {
    let rest = uri.strip_prefix("crustation://servers/")?;
    let (id, what) = rest.split_once('/')?;
    Some((id.to_string(), what))
}

const SERVERS_URI: &str = "crustation://servers";

#[tool_handler]
impl ServerHandler for Crustation {
    fn get_info(&self) -> ServerConfig {
        let mut info = Implementation::from_build_env();
        info.name = "crustation".to_string();
        info.version = env!("CARGO_PKG_VERSION").to_string();

        let mut config = ServerConfig::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .build(),
        );
        config.protocol_version = ProtocolVersion::default();
        config.server_info = info;
        config.instructions = Some(
            "Crustation is a Minecraft server control panel. Call list_servers first: every \
             other tool takes a server's id or its name. read_console is where a crash or a \
             misbehaving plugin will show itself, and send_command runs one command on a \
             server that is up. What this key may do is set by its permissions, so a tool can \
             answer that it is not allowed."
                .to_string(),
        );
        config
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, ErrorData> {
        let parts = http_parts(&context)?;
        let who = Self::caller(&parts)?;

        let mut resources = vec![
            Resource::new(SERVERS_URI, "servers")
                .with_title("Every server")
                .with_description("All the game servers this key can see, and what each is doing.")
                .with_mime_type("application/json"),
        ];

        for row in self.visible(who).await? {
            let at = format!("{SERVERS_URI}/{}", row.id);
            resources.push(
                Resource::new(format!("{at}/details"), format!("{} · details", row.name))
                    .with_description("What this server is, and what it last said about itself.")
                    .with_mime_type("application/json"),
            );
            resources.push(
                Resource::new(format!("{at}/console"), format!("{} · console", row.name))
                    .with_description("The last 200 lines of this server's console.")
                    .with_mime_type("text/plain"),
            );
            resources.push(
                Resource::new(
                    format!("{at}/properties"),
                    format!("{} · server.properties", row.name),
                )
                .with_description("This server's settings file, as it stands on disk.")
                .with_mime_type("application/json"),
            );
        }

        Ok(ListResourcesResult {
            resources,
            ..Default::default()
        })
    }

    async fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, ErrorData> {
        Ok(ListResourceTemplatesResult {
            resource_templates: vec![
                ResourceTemplate::new("crustation://servers/{id}/details", "server-details")
                    .with_description("One server, by id.")
                    .with_mime_type("application/json"),
                ResourceTemplate::new("crustation://servers/{id}/console", "server-console")
                    .with_description("The tail of one server's console, by id.")
                    .with_mime_type("text/plain"),
                ResourceTemplate::new("crustation://servers/{id}/properties", "server-properties")
                    .with_description("One server's server.properties, by id.")
                    .with_mime_type("application/json"),
            ],
            ..Default::default()
        })
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, ErrorData> {
        let parts = http_parts(&context)?;
        let who = Self::caller(&parts)?;
        let uri = request.uri.as_str();

        if uri == SERVERS_URI {
            let mut out = Vec::new();
            for row in self.visible(who).await? {
                out.push(self.summarise(&row).await);
            }
            return Ok(json_resource(uri, &out)?.into());
        }

        let (id, what) = parse_uri(uri).ok_or_else(|| {
            ErrorData::resource_not_found(
                format!("{uri} is not a resource this panel keeps."),
                None,
            )
        })?;

        let result = match what {
            "details" => {
                let row = self.find(who, &id, ServerPerm::Logs).await?;
                json_resource(uri, &self.summarise(&row).await)?
            }
            "console" => {
                let row = self.find(who, &id, ServerPerm::Console).await?;
                let text = self
                    .tail(row.uuid(), Some(200))
                    .await
                    .into_iter()
                    .map(|line| format!("{} {}", line.at, line.text))
                    .collect::<Vec<_>>()
                    .join("\n");
                ReadResourceResult::new(vec![ResourceContents::text(text, uri)])
            }
            "properties" => {
                let row = self.find(who, &id, ServerPerm::Config).await?;
                json_resource(uri, &self.properties_of(&row).await?)?
            }
            other => {
                return Err(ErrorData::resource_not_found(
                    format!("A server has no '{other}' to read."),
                    None,
                ));
            }
        };
        Ok(result.into())
    }
}

/// The HTTP request this call arrived on. Absent only if the handler is ever
/// served over a transport that has none.
fn http_parts(context: &RequestContext<RoleServer>) -> Result<Parts, ErrorData> {
    context
        .extensions
        .get::<Parts>()
        .cloned()
        .ok_or_else(|| ErrorData::internal_error("This call arrived without a request.", None))
}

fn json_resource<T: Serialize>(uri: &str, value: &T) -> Result<ReadResourceResult, ErrorData> {
    let text = serde_json::to_string_pretty(value).map_err(internal)?;
    Ok(ReadResourceResult::new(vec![
        ResourceContents::TextResourceContents {
            uri: uri.to_string(),
            mime_type: Some("application/json".to_string()),
            text,
            meta: None,
        },
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_server_uri_names_its_id_and_what_to_read() {
        assert_eq!(
            parse_uri("crustation://servers/abc/console"),
            Some(("abc".to_string(), "console"))
        );
    }

    #[test]
    fn the_catalogue_names_every_tool_and_no_others() {
        let mut router: Vec<String> = Crustation::tool_router()
            .list_all()
            .into_iter()
            .map(|tool| tool.name.to_string())
            .collect();
        let mut listed: Vec<String> = catalogue()
            .into_iter()
            .map(|(name, _)| name.to_string())
            .collect();
        router.sort();
        listed.sort();
        assert_eq!(
            router, listed,
            "a tool was added or renamed without saying what permission it needs"
        );
    }

    #[test]
    fn every_tool_is_described_for_the_panel_screen() {
        for tool in described() {
            assert!(
                tool["description"]
                    .as_str()
                    .is_some_and(|one| !one.is_empty()),
                "{} has nothing to tell the operator it does",
                tool["name"]
            );
        }
    }

    #[test]
    fn anything_else_is_not_one_of_ours() {
        assert!(parse_uri("crustation://servers").is_none());
        assert!(parse_uri("file:///etc/passwd").is_none());
        assert!(parse_uri("crustation://servers/abc").is_none());
    }
}
