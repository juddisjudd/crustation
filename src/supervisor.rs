use std::borrow::Cow;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;

use anyhow::{Context, Result, anyhow, bail};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::json;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{Mutex, RwLock};
use uuid::Uuid;

use crate::config::Config;
use crate::db::Db;
use crate::events::Events;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Stopped,
    Starting,
    Running,
    Stopping,
    Crashed,
    Installing,
}

impl State {
    pub fn as_str(self) -> &'static str {
        match self {
            State::Stopped => "stopped",
            State::Starting => "starting",
            State::Running => "running",
            State::Stopping => "stopping",
            State::Crashed => "crashed",
            State::Installing => "installing",
        }
    }

    pub fn is_live(self) -> bool {
        matches!(self, State::Starting | State::Running | State::Stopping)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ConsoleLine {
    pub seq: u64,
    pub at: DateTime<Utc>,
    pub stream: &'static str,
    pub text: String,
    /// Chatter the game prints on every start, which the interface hides until asked.
    pub noise: bool,
}

/// What the supervisor knows about one server while the process is up.
#[derive(Debug, Default, Clone, Serialize)]
pub struct Runtime {
    pub state_str: String,
    pub pid: Option<u32>,
    pub started_at: Option<DateTime<Utc>>,
    pub installing: bool,
    pub backing_up: bool,
    pub updating: bool,
}

struct Instance {
    state: State,
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    pid: Option<u32>,
    started_at: Option<DateTime<Utc>>,
    /// Bounded console backlog, newest last.
    console: Vec<ConsoleLine>,
    next_seq: u64,
    stopping: bool,
    installing: bool,
    backing_up: bool,
    updating: bool,
    /// Why Rust+ failed its connectivity test since the last start, if it did.
    rust_plus_failure: Option<String>,
}

impl Instance {
    fn new() -> Self {
        Self {
            state: State::Stopped,
            child: None,
            stdin: None,
            pid: None,
            started_at: None,
            console: Vec::new(),
            next_seq: 1,
            stopping: false,
            installing: false,
            backing_up: false,
            updating: false,
            rust_plus_failure: None,
        }
    }

    /// Appends to the ring buffer and hands back the line to publish.
    fn push(
        &mut self,
        stream: &'static str,
        text: String,
        noise: bool,
        backlog: usize,
    ) -> ConsoleLine {
        let line = ConsoleLine {
            seq: self.next_seq,
            at: Utc::now(),
            stream,
            text,
            noise,
        };
        self.next_seq += 1;
        self.console.push(line.clone());
        if self.console.len() > backlog {
            let excess = self.console.len() - backlog;
            self.console.drain(0..excess);
        }
        line
    }

    fn flags(&self) -> serde_json::Value {
        json!({
            "installing": self.installing,
            "updating": self.updating,
            "backing_up": self.backing_up,
            "crashed": self.state == State::Crashed,
        })
    }
}

/// Spawns and watches game server processes. One entry per server id.
#[derive(Clone)]
pub struct Supervisor {
    config: Config,
    db: Db,
    events: Events,
    instances: Arc<RwLock<HashMap<Uuid, Arc<Mutex<Instance>>>>>,
}

/// Everything needed to start one server, read from its database row.
pub struct Launch {
    pub id: Uuid,
    pub kind: String,
    pub directory: PathBuf,
    pub command: String,
    pub stop_command: String,
    pub shutdown_timeout: u64,
    pub crash_detection: bool,
    pub ignored_exits: Vec<i32>,
    pub console_backlog: usize,
}

impl Supervisor {
    pub fn new(config: Config, db: Db, events: Events) -> Self {
        Self {
            config,
            db,
            events,
            instances: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    async fn instance(&self, id: Uuid) -> Arc<Mutex<Instance>> {
        if let Some(found) = self.instances.read().await.get(&id) {
            return found.clone();
        }
        let mut guard = self.instances.write().await;
        guard
            .entry(id)
            .or_insert_with(|| Arc::new(Mutex::new(Instance::new())))
            .clone()
    }

    pub async fn state(&self, id: Uuid) -> State {
        self.instance(id).await.lock().await.state
    }

    pub async fn runtime(&self, id: Uuid) -> Runtime {
        let instance = self.instance(id).await;
        let guard = instance.lock().await;
        Runtime {
            state_str: guard.state.as_str().to_string(),
            pid: guard.pid,
            started_at: guard.started_at,
            installing: guard.installing,
            backing_up: guard.backing_up,
            updating: guard.updating,
        }
    }

    /// Why Rust+ failed its connectivity test since the last start; empty when Rust said nothing more.
    pub async fn rust_plus_failure(&self, id: Uuid) -> Option<String> {
        self.instance(id)
            .await
            .lock()
            .await
            .rust_plus_failure
            .clone()
    }

    pub async fn console(&self, id: Uuid, after: Option<u64>) -> Vec<ConsoleLine> {
        let instance = self.instance(id).await;
        let guard = instance.lock().await;
        match after {
            Some(seq) => guard
                .console
                .iter()
                .filter(|line| line.seq > seq)
                .cloned()
                .collect(),
            None => guard.console.clone(),
        }
    }

    /// Empties the backlog. Sequence numbers carry on from where they were, so
    /// a client polling with `after` is not sent the next line twice.
    pub async fn clear_console(&self, id: Uuid) {
        let instance = self.instance(id).await;
        instance.lock().await.console.clear();
        self.events.console_cleared(id);
    }

    /// Marks a long-running job so the interface can show it. Returns a guard-like
    /// setter the caller uses again to clear the flag.
    pub async fn set_flag(&self, id: Uuid, flag: Flag, value: bool) {
        let instance = self.instance(id).await;
        let mut guard = instance.lock().await;
        match flag {
            Flag::Installing => guard.installing = value,
            Flag::BackingUp => guard.backing_up = value,
            Flag::Updating => guard.updating = value,
        }
        if value && flag == Flag::Installing {
            guard.state = State::Installing;
        } else if !value && guard.state == State::Installing {
            guard.state = State::Stopped;
        }
        self.events
            .server_state(id, guard.state.as_str(), guard.flags());
    }

    pub async fn start(&self, launch: Launch) -> Result<()> {
        let instance = self.instance(launch.id).await;
        let mut guard = instance.lock().await;
        if guard.state.is_live() {
            bail!("server is already running");
        }
        if launch.command.trim().is_empty() {
            bail!("this server has no start command");
        }

        tokio::fs::create_dir_all(&launch.directory)
            .await
            .with_context(|| format!("creating {}", launch.directory.display()))?;
        if launch.kind == crate::dragonwilds::KIND {
            crate::dragonwilds::before_start(&launch.directory)
                .await
                .context("putting the admin password back")?;
        }

        let parts = shell_words::split(&launch.command).context("parsing the start command")?;
        let (program, args) = parts
            .split_first()
            .ok_or_else(|| anyhow!("the start command is empty"))?;

        let mut command = Command::new(program);
        command
            .args(args)
            .current_dir(&launch.directory)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(false);
        // Its own group, so a stop reaches the game behind a wrapper such as Wine or a launch script.
        #[cfg(unix)]
        command.process_group(0);
        let mut child = command
            .spawn()
            .with_context(|| format!("starting {program}"))?;

        let stdout = child.stdout.take().expect("stdout was piped");
        let stderr = child.stderr.take().expect("stderr was piped");
        guard.stdin = child.stdin.take();
        guard.pid = child.id();
        guard.started_at = Some(Utc::now());
        guard.state = State::Starting;
        guard.rust_plus_failure = None;
        guard.stopping = false;
        guard.child = Some(child);
        let flags = guard.flags();
        drop(guard);

        self.events
            .server_state(launch.id, State::Starting.as_str(), flags);

        self.pump(
            launch.id,
            instance.clone(),
            stdout,
            "stdout",
            launch.kind.clone(),
            launch.console_backlog,
        );
        self.pump(
            launch.id,
            instance.clone(),
            stderr,
            "stderr",
            launch.kind.clone(),
            launch.console_backlog,
        );
        self.watch_exit(launch, instance);
        Ok(())
    }

    fn pump<R>(
        &self,
        id: Uuid,
        instance: Arc<Mutex<Instance>>,
        reader: R,
        stream: &'static str,
        kind: String,
        backlog: usize,
    ) where
        R: tokio::io::AsyncRead + Unpin + Send + 'static,
    {
        let events = self.events.clone();
        let db = self.db.clone();
        tokio::spawn(async move {
            let rust = kind == crate::rust_game::KIND;
            let dragonwilds = kind == crate::dragonwilds::KIND;
            let mut lines = BufReader::new(reader).lines();
            let mut chatter = rust.then(crate::rust_console::Noise::default);
            while let Ok(Some(text)) = lines.next_line().await {
                let text = strip_escapes(&text);
                let text = match dragonwilds {
                    true => crate::dragonwilds::redact(&text).into_owned(),
                    false => text.into_owned(),
                };
                match arrival_in(&kind, &text) {
                    Some(Seen::Arrived(name, player_id)) => {
                        mark_arrived(&db, id, &name, player_id.as_deref()).await;
                    }
                    Some(Seen::Left(name)) => mark_left(&db, id, &name).await,
                    None if rust => {
                        if let Some(url) = crate::rust_console::map_image(&text) {
                            remember_map_image(&db, id, url).await;
                        }
                    }
                    None => {}
                }
                let noise = chatter.as_mut().is_some_and(|chatter| chatter.hears(&text))
                    || (dragonwilds && crate::dragonwilds::is_noise(&text));
                let mut guard = instance.lock().await;
                guard.rust_plus_failure = rust_plus_after(guard.rust_plus_failure.take(), &text);
                let line = guard.push(stream, text, noise, backlog);
                // A server that prints "Done" is up; good enough across the games we run.
                if guard.state == State::Starting && looks_ready(&line.text) {
                    guard.state = State::Running;
                    let flags = guard.flags();
                    drop(guard);
                    events.server_state(id, State::Running.as_str(), flags);
                } else {
                    drop(guard);
                }
                events.console_line(id, serde_json::to_value(&line).unwrap_or_default());
            }
        });
    }

    fn watch_exit(&self, launch: Launch, instance: Arc<Mutex<Instance>>) {
        let events = self.events.clone();
        let supervisor = self.clone();
        tokio::spawn(async move {
            let child = {
                let mut guard = instance.lock().await;
                guard.child.take()
            };
            let Some(mut child) = child else { return };
            let status = child.wait().await;

            let mut guard = instance.lock().await;
            let code = status.as_ref().ok().and_then(|s| s.code()).unwrap_or(-1);
            let requested = guard.stopping;
            let clean = requested || launch.ignored_exits.contains(&code);

            guard.child = None;
            guard.stdin = None;
            guard.pid = None;
            guard.started_at = None;
            guard.stopping = false;
            guard.state = if clean {
                State::Stopped
            } else {
                State::Crashed
            };
            let state = guard.state;
            let flags = guard.flags();
            drop(guard);

            // Nobody is on a server that has stopped. The marks are written as
            // people arrive, so without this they outlive the process that
            // earned them and the list still says two are playing.
            forget_who_was_on(&supervisor.db, launch.id).await;

            events.server_state(launch.id, state.as_str(), flags);
            if !clean {
                events.notify(
                    "error",
                    format!("A server stopped unexpectedly (exit code {code})."),
                );
                if launch.crash_detection {
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    if let Err(error) = supervisor.start_by_id(launch.id).await {
                        tracing::warn!(%error, server = %launch.id, "restart after crash failed");
                    }
                }
            }
        });
    }

    /// Loads the row and starts it. Used by autostart, schedules and crash recovery.
    pub async fn start_by_id(&self, id: Uuid) -> Result<()> {
        let launch = self.launch_for(id).await?;
        self.start(launch).await
    }

    pub async fn launch_for(&self, id: Uuid) -> Result<Launch> {
        let row: (String, String, String, String, i64, bool, String) = sqlx::query_as(
            "SELECT kind, directory, command, stop_command, shutdown_timeout, crash_detection,
                    ignored_exits
             FROM servers WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_one(&self.db)
        .await?;

        Ok(Launch {
            id,
            kind: row.0,
            directory: PathBuf::from(row.1),
            command: row.2,
            stop_command: row.3,
            shutdown_timeout: row.4.max(0) as u64,
            crash_detection: row.5,
            ignored_exits: row
                .6
                .split(',')
                .filter_map(|code| code.trim().parse().ok())
                .collect(),
            console_backlog: self.config.panel.console_backlog,
        })
    }

    pub async fn send(&self, id: Uuid, command: &str) -> Result<()> {
        let instance = self.instance(id).await;
        let mut guard = instance.lock().await;
        if !guard.state.is_live() {
            bail!("the server is not running");
        }
        let stdin = guard
            .stdin
            .as_mut()
            .ok_or_else(|| anyhow!("this server does not accept commands"))?;
        stdin.write_all(command.as_bytes()).await?;
        stdin.write_all(b"\n").await?;
        stdin.flush().await?;
        Ok(())
    }

    /// Adds a line the panel produced, so everyone watching the console sees it.
    pub async fn push_console(&self, id: Uuid, stream: &'static str, text: String) {
        self.push_judged(id, stream, text, false).await;
    }

    /// The same, for a line whose sender knows whether it is chatter.
    pub async fn push_judged(&self, id: Uuid, stream: &'static str, text: String, noise: bool) {
        let instance = self.instance(id).await;
        let line = {
            let mut guard = instance.lock().await;
            guard.push(stream, text, noise, self.config.panel.console_backlog)
        };
        self.events
            .console_line(id, serde_json::to_value(&line).unwrap_or_default());
    }

    /// Runs a command over RCON when the server has it set up, and over stdin
    /// otherwise. Only RCON hands an answer back.
    pub async fn run_command(&self, id: Uuid, command: &str) -> Result<Outcome> {
        let kind: String = sqlx::query_scalar("SELECT kind FROM servers WHERE id = ?")
            .bind(id.to_string())
            .fetch_one(&self.db)
            .await
            .unwrap_or_default();
        if !crate::steam_games::takes_commands(&kind) {
            bail!("this game reads no commands, from its console or anywhere else");
        }
        self.run(id, command, true).await
    }

    async fn run(&self, id: Uuid, command: &str, echo: bool) -> Result<Outcome> {
        if !self.state(id).await.is_live() {
            bail!("the server is not running");
        }

        if let Some((endpoint, listener)) = self.rcon_endpoint(id).await {
            let rust = listener != Listener::Minecraft;
            let answer = match listener {
                Listener::RustWeb => crate::webrcon::run(&endpoint, command).await,
                _ => crate::rcon::run(&endpoint, command).await,
            };
            match answer {
                Ok(output) => {
                    let output = output.trim_end().to_string();
                    // Minecraft does not log RCON commands, so echo both sides; Rust logs them itself.
                    if echo && !rust {
                        self.push_console(id, "command", format!("> {command}"))
                            .await;
                        for text in output.lines() {
                            self.push_console(id, "rcon", text.to_string()).await;
                        }
                    }
                    return Ok(Outcome {
                        via: "rcon",
                        output: Some(output),
                    });
                }
                // Rust ignores stdin, so a typed command RCON could not deliver has to say so.
                Err(error) if echo && rust => {
                    return Err(error.context("RCON did not take the command"));
                }
                Err(error) => {
                    tracing::warn!(%error, server = %id, "RCON failed, falling back to stdin");
                }
            }
        }

        self.send(id, command).await?;
        Ok(Outcome {
            via: "stdin",
            output: None,
        })
    }

    /// Asks a Rust server something over websocket RCON, which leaves its console alone.
    pub async fn ask(&self, id: Uuid, command: &str) -> Result<String> {
        if !self.state(id).await.is_live() {
            bail!("the server is not running");
        }
        match self.rcon_endpoint(id).await {
            Some((endpoint, Listener::RustWeb)) => crate::webrcon::run(&endpoint, command).await,
            _ => bail!("only a Rust server on websocket RCON can be asked quietly"),
        }
    }

    /// Chat from a Rust server on websocket RCON as it is said.
    pub async fn chat_feed(
        &self,
        id: Uuid,
    ) -> Result<(
        tokio::sync::broadcast::Receiver<String>,
        Arc<std::sync::atomic::AtomicBool>,
    )> {
        match self.rcon_endpoint(id).await {
            Some((endpoint, Listener::RustWeb)) => crate::webrcon::listen(&endpoint).await,
            _ => bail!("only a Rust server on websocket RCON pushes its chat"),
        }
    }

    async fn rcon_endpoint(&self, id: Uuid) -> Option<(crate::rcon::Endpoint, Listener)> {
        let row: (String, String, String) =
            sqlx::query_as("SELECT directory, kind, command FROM servers WHERE id = ?")
                .bind(id.to_string())
                .fetch_one(&self.db)
                .await
                .ok()?;
        // Rust's listener is named in its start command.
        if row.1 == crate::rust_game::KIND {
            let listener = match crate::rust_game::rcon_web(&row.2) {
                true => Listener::RustWeb,
                false => Listener::RustLegacy,
            };
            return crate::rust_game::rcon_endpoint(&row.2).map(|endpoint| (endpoint, listener));
        }
        // Bedrock has no RCON, so stdin stays its only command path.
        if row.1 != "minecraft_java" {
            return None;
        }
        crate::rcon::endpoint(Path::new(&row.0))
            .await
            .ok()
            .flatten()
            .map(|endpoint| (endpoint, Listener::Minecraft))
    }

    /// Asks politely, waits, then kills.
    pub async fn stop(&self, id: Uuid) -> Result<()> {
        let launch = self.launch_for(id).await?;
        let instance = self.instance(id).await;
        {
            let mut guard = instance.lock().await;
            if !guard.state.is_live() {
                bail!("the server is not running");
            }
            guard.stopping = true;
            guard.state = State::Stopping;
            let flags = guard.flags();
            drop(guard);
            self.events
                .server_state(id, State::Stopping.as_str(), flags);
        }

        if !launch.stop_command.trim().is_empty() {
            // Over RCON where there is one, since Rust does not reliably read its stdin.
            let _ = self.run(id, &launch.stop_command, false).await;
        } else if let Some(pid) = self.pid(id).await {
            interrupt(pid);
        }

        let deadline =
            tokio::time::Instant::now() + std::time::Duration::from_secs(launch.shutdown_timeout);
        loop {
            if !self.state(id).await.is_live() {
                return Ok(());
            }
            if tokio::time::Instant::now() >= deadline {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        }
        tracing::warn!(server = %id, "stop timed out, killing the process");
        self.kill(id).await
    }

    pub async fn kill(&self, id: Uuid) -> Result<()> {
        let instance = self.instance(id).await;
        let mut guard = instance.lock().await;
        guard.stopping = true;
        // By pid, so whatever the server started goes with it.
        if let Some(pid) = guard.pid {
            kill_pid(pid);
        } else if let Some(child) = guard.child.as_mut() {
            child.start_kill().ok();
        }
        Ok(())
    }

    pub async fn restart(&self, id: Uuid) -> Result<()> {
        if self.state(id).await.is_live() {
            self.stop(id).await?;
        }
        self.start_by_id(id).await
    }

    /// Stops everything still running, for a clean shutdown.
    pub async fn stop_all(&self) {
        // All at once, so the wait is the slowest save rather than the sum of them, inside Docker's stop timeout.
        let running = self.running_ids().await;
        futures::future::join_all(running.into_iter().map(|id| self.stop(id))).await;
    }

    pub async fn running_ids(&self) -> Vec<Uuid> {
        let mut out = Vec::new();
        for (id, instance) in self.instances.read().await.iter() {
            if instance.lock().await.state.is_live() {
                out.push(*id);
            }
        }
        out
    }

    pub async fn pid(&self, id: Uuid) -> Option<u32> {
        self.instance(id).await.lock().await.pid
    }
}

/// How a command reached the server, and anything it said back.
pub struct Outcome {
    pub via: &'static str,
    pub output: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flag {
    Installing,
    #[allow(dead_code, reason = "used by backups")]
    BackingUp,
    #[allow(dead_code, reason = "used by guarded upgrades")]
    Updating,
}

/// Bedrock announces an arrival as
/// `Player connected: ohitsjudd, xuid: 2535458356136740`. It is the only time
/// the server says an Xbox id, and permissions.json is keyed by nothing else,
/// so the panel writes it down when it goes past.
fn xuid_in(line: &str) -> Option<(String, String)> {
    let (_, rest) = line.split_once("Player connected:")?;
    let (name, tail) = rest.split_once(", xuid:")?;
    let name = name.trim();
    let xuid = tail.trim();
    let usable = !name.is_empty() && xuid.len() >= 10 && xuid.chars().all(|c| c.is_ascii_digit());
    usable.then(|| (name.to_string(), xuid.to_string()))
}

/// The other half of the pair: `Player disconnected: ohitsjudd, xuid: …`.
/// Without it a Bedrock arrival is never taken back, since the pong that would
/// otherwise say who is on carries no names at all.
fn left_in(line: &str) -> Option<String> {
    let (_, rest) = line.split_once("Player disconnected:")?;
    let name = rest.split(", xuid:").next()?.trim();
    (!name.is_empty()).then(|| name.to_string())
}

/// Somebody the console said came or went.
#[derive(Debug, PartialEq)]
enum Seen {
    Arrived(String, Option<String>),
    Left(String),
}

/// Bedrock, Enshrouded and Dragonwilds each announce comings and goings in their own words.
fn arrival_in(kind: &str, line: &str) -> Option<Seen> {
    match kind {
        crate::enshrouded::KIND => crate::enshrouded::arrival(line)
            .map(|name| Seen::Arrived(name, None))
            .or_else(|| crate::enshrouded::departure(line).map(Seen::Left)),
        crate::dragonwilds::KIND => crate::dragonwilds::arrival(line)
            .map(|(player, name)| Seen::Arrived(name, Some(player)))
            .or_else(|| crate::dragonwilds::departure(line).map(|(_, name)| Seen::Left(name))),
        _ => xuid_in(line)
            .map(|(name, xuid)| Seen::Arrived(name, Some(xuid)))
            .or_else(|| left_in(line).map(Seen::Left)),
    }
}

/// Marks somebody on, keeping any id the line gave in the column Java uses for its own.
async fn mark_arrived(db: &crate::db::Db, id: Uuid, name: &str, player_id: Option<&str>) {
    let now = chrono::Utc::now().to_rfc3339();
    let result = sqlx::query(
        "INSERT INTO server_players (server_id, name, uuid, first_seen, last_seen, online)
         VALUES (?, ?, ?, ?, ?, 1)
         ON CONFLICT(server_id, name) DO UPDATE SET
             uuid = COALESCE(excluded.uuid, server_players.uuid),
             last_seen = excluded.last_seen,
             online = 1",
    )
    .bind(id.to_string())
    .bind(name)
    .bind(player_id)
    .bind(&now)
    .bind(&now)
    .execute(db)
    .await;
    if let Err(error) = result {
        tracing::debug!(%error, "could not mark somebody as on");
    }
}

/// Marks one person off, by name, leaving the rest of the row alone.
async fn mark_left(db: &crate::db::Db, id: Uuid, name: &str) {
    let now = chrono::Utc::now().to_rfc3339();
    let result = sqlx::query(
        "UPDATE server_players SET online = 0, last_seen = ?
         WHERE server_id = ? AND name = ?",
    )
    .bind(&now)
    .bind(id.to_string())
    .bind(name)
    .execute(db)
    .await;
    if let Err(error) = result {
        tracing::debug!(%error, "could not mark somebody as gone");
    }
}

/// Kept on the row, in case a start from a save does not link the picture again.
async fn remember_map_image(db: &crate::db::Db, id: Uuid, url: &str) {
    let result = sqlx::query("UPDATE servers SET map_image = ? WHERE id = ?")
        .bind(url)
        .bind(id.to_string())
        .execute(db)
        .await;
    if let Err(error) = result {
        tracing::debug!(%error, "could not write down the map's picture");
    }
}

/// Nobody is on a server that is not running.
async fn forget_who_was_on(db: &crate::db::Db, id: Uuid) {
    let result = sqlx::query("UPDATE server_players SET online = 0 WHERE server_id = ?")
        .bind(id.to_string())
        .execute(db)
        .await;
    if let Err(error) = result {
        tracing::debug!(%error, "could not clear the online marks");
    }
}

/// Which RCON a server speaks.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Listener {
    Minecraft,
    RustLegacy,
    RustWeb,
}

/// Terminal colour and cursor codes, which a browser would print as junk.
pub fn strip_escapes(text: &str) -> Cow<'_, str> {
    if !text.contains('\u{1b}') {
        return Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('[') => {
                for c in chars.by_ref() {
                    if ('@'..='~').contains(&c) {
                        break;
                    }
                }
            }
            Some(']') => {
                while let Some(c) = chars.next() {
                    if c == '\u{7}' {
                        break;
                    }
                    if c == '\u{1b}' {
                        chars.next();
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    Cow::Owned(out)
}

/// What Rust+'s connectivity test has said so far, after one more console line.
fn rust_plus_after(known: Option<String>, line: &str) -> Option<String> {
    if line.contains("companion server connectivity test failed") {
        Some(String::new())
    } else if known.is_some() && line.starts_with("Could not establish a TCP connection to") {
        Some(line.to_string())
    } else {
        known
    }
}

fn looks_ready(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.contains("done (")
        || lower.contains("server started")
        || lower.contains("for help, type")
        || lower.contains("server startup complete")
        // Enshrouded once its world is loaded, then Dragonwilds loading its world or advertising a new one.
        || lower.contains("[game_server] switching state from loadecsscene to run")
        || lower.contains("world load succeeded")
        || lower.contains("create session - advertise")
}

/// Ctrl+C to the server's whole group, which is how a game that reads no commands is asked to stop.
#[cfg(unix)]
fn interrupt(pid: u32) {
    unsafe {
        libc::kill(-(pid as i32), libc::SIGINT);
    }
}

#[cfg(windows)]
fn interrupt(_pid: u32) {}

#[cfg(unix)]
fn kill_pid(pid: u32) {
    unsafe {
        libc::kill(-(pid as i32), libc::SIGKILL);
        libc::kill(pid as i32, libc::SIGKILL);
    }
}

#[cfg(windows)]
fn kill_pid(pid: u32) {
    let _ = std::process::Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .output();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notices_rust_plus_failing_its_connectivity_test() {
        let lines = [
            "Checking for new Steam Item Definitions..",
            "Rust+ companion server connectivity test failed! Disabling Rust+ features.",
            "",
            "Could not establish a TCP connection to 203.0.113.7:28083",
            "Curl error 7: Failed to connect",
        ];
        let known = lines
            .iter()
            .fold(None, |known, line| rust_plus_after(known, line));
        assert_eq!(
            known.as_deref(),
            Some("Could not establish a TCP connection to 203.0.113.7:28083")
        );
        let quiet = [
            "Server startup complete",
            "Could not establish a TCP connection to x",
        ];
        assert_eq!(
            quiet
                .iter()
                .fold(None, |known, line| rust_plus_after(known, line)),
            None
        );
    }

    #[test]
    fn terminal_codes_are_taken_out_of_a_line() {
        assert_eq!(
            strip_escapes("\x1b[0mOK \x1b[1;32mgreen\x1b[0m"),
            "OK green"
        );
        assert_eq!(strip_escapes("\x1b]0;title\x07after"), "after");
        assert!(matches!(strip_escapes("plain"), Cow::Borrowed("plain")));
    }

    #[test]
    fn clearing_the_backlog_does_not_rewind_the_sequence() {
        let mut instance = Instance::new();
        for _ in 0..3 {
            instance.push("stdout", "hello".to_string(), false, 500);
        }
        instance.console.clear();
        let next = instance.push("stdout", "after".to_string(), false, 500);
        assert_eq!(instance.console.len(), 1);
        assert_eq!(next.seq, 4);
    }

    #[test]
    fn the_backlog_never_grows_past_its_bound() {
        let mut instance = Instance::new();
        for _ in 0..10 {
            instance.push("stdout", "hello".to_string(), false, 4);
        }
        assert_eq!(instance.console.len(), 4);
        assert_eq!(instance.console[0].seq, 7);
    }

    #[test]
    fn a_bedrock_arrival_gives_up_its_xbox_id() {
        let line =
            "[2026-09-20 12:34:56:789 INFO] Player connected: ohitsjudd, xuid: 2535458356136740";
        assert_eq!(
            xuid_in(line),
            Some(("ohitsjudd".to_string(), "2535458356136740".to_string()))
        );
    }

    #[test]
    fn a_name_with_a_space_in_it_still_reads() {
        let line =
            "[2026-09-20 12:34:56:789 INFO] Player connected: Some One, xuid: 2535000000000001";
        assert_eq!(
            xuid_in(line),
            Some(("Some One".to_string(), "2535000000000001".to_string()))
        );
    }

    #[test]
    fn anything_else_gives_up_nothing() {
        assert!(xuid_in("[INFO] Player disconnected: ohitsjudd, xuid: 2535458356136740").is_none());
        assert!(xuid_in("[INFO] Server started.").is_none());
        assert!(xuid_in("Player connected: nobody, xuid: not-a-number").is_none());
        assert!(xuid_in("Player connected: , xuid: 2535458356136740").is_none());
    }

    #[test]
    fn a_departure_is_read_back_as_the_name_alone() {
        let line = "[2026-09-20 18:54:38:949 INFO] Player disconnected: ohitsjudd, \
                    xuid: 2535458356136740, pfid: 3BDCCD5EB8F51F1E";
        assert_eq!(left_in(line), Some("ohitsjudd".to_string()));
        assert_eq!(
            left_in("[INFO] Player disconnected: Some One, xuid: 2535000000000001"),
            Some("Some One".to_string())
        );
    }

    #[test]
    fn an_arrival_is_not_mistaken_for_a_departure() {
        assert!(left_in("[INFO] Player connected: ohitsjudd, xuid: 2535458356136740").is_none());
        assert!(left_in("[INFO] Server started.").is_none());
        assert!(left_in("[INFO] Player disconnected: , xuid: 2535458356136740").is_none());
    }

    #[test]
    fn a_server_that_says_done_is_up() {
        assert!(looks_ready(
            "[12:00:00] [Server thread/INFO]: Done (4.5s)! For help, type \"help\""
        ));
        assert!(looks_ready("[INFO] Server started."));
        assert!(!looks_ready("[INFO] Preparing level"));
    }

    #[test]
    fn the_steam_games_say_they_are_up_in_their_own_words() {
        assert!(looks_ready(
            "[game_server] Switching state from LoadEcsScene to Run after 488.41 ms"
        ));
        assert!(!looks_ready("[Session] 'HostOnline' (up)!"));
        assert!(looks_ready(
            "[2026.09.28-10.00.00:000][  0]LogPersistence: [DedicatedServer] PostLoadWorldState() : World load SUCCEEDED (slot: Test World)"
        ));
        assert!(looks_ready(
            "LogDomMatcherSession: CREATE SESSION - Advertise[1]"
        ));
    }

    #[test]
    fn each_game_announces_its_players_its_own_way() {
        assert_eq!(
            arrival_in(
                crate::enshrouded::KIND,
                "[server] Player 'Wanderer' logged in with Permissions:"
            ),
            Some(Seen::Arrived("Wanderer".into(), None))
        );
        assert_eq!(
            arrival_in(crate::enshrouded::KIND, "[server] Remove Player 'Wanderer'"),
            Some(Seen::Left("Wanderer".into()))
        );
        let id = "0123456789abcdef0123456789abcdef";
        assert_eq!(
            arrival_in(
                crate::dragonwilds::KIND,
                &format!("LogDomMatcherSession: Player ADDED to session [{id}]-[Wanderer]")
            ),
            Some(Seen::Arrived("Wanderer".into(), Some(id.into())))
        );
        assert_eq!(
            arrival_in(
                "minecraft_bedrock",
                "Player connected: ohitsjudd, xuid: 2535458356136740"
            ),
            Some(Seen::Arrived(
                "ohitsjudd".into(),
                Some("2535458356136740".into())
            ))
        );
        // Each game reads only its own lines.
        assert_eq!(
            arrival_in(
                crate::dragonwilds::KIND,
                "[server] Player 'Wanderer' logged in with Permissions:"
            ),
            None
        );
    }
}
