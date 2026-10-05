//! Rust's players: who is on, who is staff, who is banned, and what was said.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Where Rust keeps its saves, under the identity the command names.
pub fn identity_dir(directory: &Path, command: &str) -> PathBuf {
    let identity = shell_words::split(command)
        .ok()
        .and_then(|words| {
            let at = words.iter().position(|word| word == "+server.identity")?;
            words.get(at + 1).cloned()
        })
        .unwrap_or_else(|| "crustation".into());
    directory.join("server").join(identity)
}

/// Where Rust keeps `users.cfg` and `bans.cfg`.
pub fn cfg_dir(directory: &Path, command: &str) -> PathBuf {
    identity_dir(directory, command).join("cfg")
}

/// A SteamID64: seventeen digits, starting 7656.
pub fn is_steam_id(value: &str) -> bool {
    value.len() == 17 && value.starts_with("7656") && value.bytes().all(|b| b.is_ascii_digit())
}

/// One row of `playerlist`.
#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub struct Online {
    #[serde(rename(deserialize = "SteamID"))]
    pub steam_id: String,
    #[serde(rename(deserialize = "DisplayName"))]
    pub name: String,
    #[serde(rename(deserialize = "Ping"), default)]
    pub ping: i64,
    #[serde(rename(deserialize = "Address"), default)]
    pub address: String,
    #[serde(rename(deserialize = "ConnectedSeconds"), default)]
    pub connected_seconds: i64,
    #[serde(rename(deserialize = "Health"), default)]
    pub health: f64,
}

pub fn parse_online(json: &str) -> Vec<Online> {
    serde_json::from_str(json).unwrap_or_default()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Owner,
    Moderator,
}

impl Role {
    pub fn command(self) -> &'static str {
        match self {
            Role::Owner => "ownerid",
            Role::Moderator => "moderatorid",
        }
    }

    pub fn removal(self) -> &'static str {
        match self {
            Role::Owner => "removeowner",
            Role::Moderator => "removemoderator",
        }
    }
}

#[derive(Debug, Serialize, PartialEq)]
pub struct Staff {
    pub steam_id: String,
    pub name: String,
    pub role: Role,
}

/// `ownerid 7656… "Name" "reason"`, one per line, as `users.cfg` holds them.
pub fn parse_staff(text: &str) -> Vec<Staff> {
    text.lines()
        .filter_map(|line| {
            let words = shell_words::split(line).ok()?;
            let role = match words.first()?.as_str() {
                "ownerid" => Role::Owner,
                "moderatorid" => Role::Moderator,
                _ => return None,
            };
            Some(Staff {
                steam_id: words.get(1)?.clone(),
                name: words.get(2).cloned().unwrap_or_default(),
                role,
            })
        })
        .collect()
}

#[derive(Debug, Serialize, PartialEq)]
pub struct Ban {
    pub steam_id: String,
    pub name: String,
    pub reason: String,
    /// Unix seconds, or `None` for a ban that never ends.
    pub expires: Option<i64>,
}

/// `banid 7656… "Name" "reason" <expiry>`, where an expiry of -1 means for good.
pub fn parse_bans(text: &str) -> Vec<Ban> {
    text.lines()
        .filter_map(|line| {
            let words = shell_words::split(line).ok()?;
            if words.first()? != "banid" {
                return None;
            }
            let expires = words
                .get(4)
                .and_then(|value| value.parse::<i64>().ok())
                .filter(|value| *value > 0);
            Some(Ban {
                steam_id: words.get(1)?.clone(),
                name: words.get(2).cloned().unwrap_or_default(),
                reason: words.get(3).cloned().unwrap_or_default(),
                expires,
            })
        })
        .collect()
}

#[derive(Deserialize)]
struct LiveBan {
    steamid: u64,
    #[serde(default)]
    username: String,
    #[serde(default)]
    notes: String,
    #[serde(default)]
    expiry: i64,
}

/// The `bans` command's JSON, which a running server answers before `bans.cfg` is saved.
pub fn parse_live_bans(json: &str) -> Option<Vec<Ban>> {
    let live: Vec<LiveBan> = serde_json::from_str(json).ok()?;
    Some(
        live.into_iter()
            .map(|ban| Ban {
                steam_id: ban.steamid.to_string(),
                name: ban.username,
                reason: ban.notes,
                expires: Some(ban.expiry).filter(|expiry| *expiry > 0),
            })
            .collect(),
    )
}

/// A cfg file with every line about one player taken out, and `line` added when given.
pub fn with_line(text: &str, verbs: &[&str], steam_id: &str, line: Option<String>) -> String {
    let mut out: Vec<String> = text
        .lines()
        .filter(|existing| {
            let words = shell_words::split(existing).unwrap_or_default();
            let about = words
                .first()
                .is_some_and(|verb| verbs.contains(&verb.as_str()))
                && words.get(1).is_some_and(|id| id == steam_id);
            !about && !existing.trim().is_empty()
        })
        .map(str::to_string)
        .collect();
    out.extend(line);
    // Rust writes these files with Windows line endings, on every platform.
    out.iter().map(|line| format!("{line}\r\n")).collect()
}

/// How long a ban lasts, as Rust takes it: a number of hours or days.
pub fn duration_seconds(value: &str) -> Option<i64> {
    let (number, unit) = value.split_at(value.len().checked_sub(1)?);
    let number: i64 = number.parse().ok().filter(|n| *n > 0)?;
    match unit {
        "h" => Some(number * 3600),
        "d" => Some(number * 86_400),
        _ => None,
    }
}

/// Keeps the roll of who has been on, with the Steam id where Minecraft keeps a UUID.
pub async fn remember(db: &crate::db::Db, id: uuid::Uuid, online: &[Online]) {
    let now = chrono::Utc::now().to_rfc3339();
    let cleared = sqlx::query("UPDATE server_players SET online = 0 WHERE server_id = ?")
        .bind(id.to_string())
        .execute(db)
        .await;
    if let Err(error) = cleared {
        tracing::debug!(%error, "could not clear the online marks");
        return;
    }
    for player in online {
        let stored = sqlx::query(
            "INSERT INTO server_players (server_id, name, uuid, first_seen, last_seen, online)
             VALUES (?, ?, ?, ?, ?, 1)
             ON CONFLICT(server_id, name) DO UPDATE SET
                 last_seen = excluded.last_seen,
                 online = 1,
                 uuid = excluded.uuid",
        )
        .bind(id.to_string())
        .bind(&player.name)
        .bind(&player.steam_id)
        .bind(&now)
        .bind(&now)
        .execute(db)
        .await;
        if let Err(error) = stored {
            tracing::debug!(%error, "could not remember a player");
        }
    }
}

/// What `serverinfo` says about how the server is coping, beyond CPU and memory.
#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub struct Health {
    #[serde(rename(deserialize = "Framerate"), default)]
    pub framerate: f64,
    #[serde(rename(deserialize = "EntityCount"), default)]
    pub entities: i64,
    #[serde(rename(deserialize = "Queued"), default)]
    pub queued: i64,
    #[serde(rename(deserialize = "Joining"), default)]
    pub joining: i64,
    #[serde(rename(deserialize = "NetworkIn"), default)]
    pub network_in: i64,
    #[serde(rename(deserialize = "NetworkOut"), default)]
    pub network_out: i64,
    #[serde(rename(deserialize = "GameTime"), default)]
    pub game_time: String,
}

/// `app.info`'s `Listening on:` and `App connects to:` lines.
pub fn parse_app_info(text: &str) -> (Option<String>, Option<String>) {
    let field = |name: &str| {
        text.lines()
            .find_map(|line| line.trim().strip_prefix(name))
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    };
    (field("Listening on:"), field("App connects to:"))
}

pub fn parse_health(json: &str) -> Option<Health> {
    serde_json::from_str(json).ok()
}

static RELAYING: std::sync::LazyLock<std::sync::Mutex<std::collections::HashSet<uuid::Uuid>>> =
    std::sync::LazyLock::new(Default::default);

/// Starts passing a running server's chat on to the console topic, unless that is happening.
pub fn relay_chat(state: &crate::state::AppState, id: uuid::Uuid) {
    if !RELAYING.lock().expect("lock").insert(id) {
        return;
    }
    let state = state.clone();
    tokio::spawn(async move {
        relay(&state, id).await;
        RELAYING.lock().expect("lock").remove(&id);
    });
}

async fn relay(state: &crate::state::AppState, id: uuid::Uuid) {
    use std::sync::atomic::Ordering;
    use tokio::sync::broadcast::error::RecvError;

    let Ok((mut said, alive)) = state.supervisor.chat_feed(id).await else {
        return;
    };
    loop {
        match tokio::time::timeout(std::time::Duration::from_secs(30), said.recv()).await {
            Ok(Ok(message)) => {
                if let Ok(line) = serde_json::from_str::<ChatLine>(&message) {
                    let line = serde_json::to_value(line).unwrap_or_default();
                    state
                        .events
                        .publish(crate::events::Topic::Console(id), "chat", line);
                }
            }
            Ok(Err(RecvError::Lagged(_))) => {}
            Ok(Err(RecvError::Closed)) => return,
            // A socket the server dropped sends nothing more; the next sample starts afresh.
            Err(_) if !alive.load(Ordering::SeqCst) => return,
            Err(_) => {}
        }
    }
}

/// One line of `chat.tail`.
#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub struct ChatLine {
    #[serde(rename(deserialize = "Channel"), default)]
    pub channel: i64,
    #[serde(rename(deserialize = "Message"))]
    pub message: String,
    #[serde(rename(deserialize = "UserId"), default)]
    pub user_id: String,
    #[serde(rename(deserialize = "Username"), default)]
    pub username: String,
    #[serde(rename(deserialize = "Time"), default)]
    pub time: i64,
}

pub fn parse_chat(json: &str) -> Vec<ChatLine> {
    serde_json::from_str(json).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_cfg_folder_under_the_identity() {
        let command = "./RustDedicated +server.identity my_server +server.port 28015";
        assert_eq!(
            cfg_dir(Path::new("srv"), command),
            Path::new("srv")
                .join("server")
                .join("my_server")
                .join("cfg")
        );
    }

    #[test]
    fn reads_who_is_on() {
        let json = r#"[{"SteamID":"76561198000000001","OwnerSteamID":"0","DisplayName":"Alice","Ping":42,"Address":"1.2.3.4:5678","ConnectedSeconds":90,"VoiationLevel":0.0,"CurrentLevel":0.0,"UnspentXp":0.0,"Health":87.5}]"#;
        let online = parse_online(json);
        assert_eq!(online.len(), 1);
        assert_eq!(online[0].name, "Alice");
        assert_eq!(online[0].ping, 42);
        assert_eq!(parse_online("[]"), Vec::new());
    }

    #[test]
    fn reads_staff_and_bans_as_rust_writes_them() {
        let users = "ownerid 76561190000000003 \"Probe Three\" \"panel test\"\r\nmoderatorid 76561190000000004 \"Probe Four\" \"\"\r\n";
        let staff = parse_staff(users);
        assert_eq!(staff.len(), 2);
        assert_eq!(staff[0].role, Role::Owner);
        assert_eq!(staff[1].name, "Probe Four");

        let bans = "banid 76561190000000001 \"Probe One\" \"testing\" -1\r\nbanid 76561190000000002 \"Probe Two\" \"timed\" 1804126118\r\n";
        let bans = parse_bans(bans);
        assert_eq!(bans[0].expires, None);
        assert_eq!(bans[1].expires, Some(1_804_126_118));
        assert_eq!(bans[1].reason, "timed");
    }

    #[test]
    fn reads_the_live_ban_list() {
        let json = r#"[{"steamid": 76561190000000001, "group": "Banned", "username": "Probe One", "notes": "testing", "expiry": -1}, {"steamid": 76561190000000002, "group": "Banned", "username": "Probe Two", "notes": "timed", "expiry": 1804126118}]"#;
        let bans = parse_live_bans(json).unwrap();
        assert_eq!(bans[0].steam_id, "76561190000000001");
        assert_eq!(bans[0].expires, None);
        assert_eq!(bans[1].expires, Some(1_804_126_118));
        assert!(parse_live_bans("not json").is_none());
    }

    #[test]
    fn replaces_the_line_about_one_player() {
        let before =
            "banid 76561190000000001 \"A\" \"x\" -1\r\nbanid 76561190000000002 \"B\" \"y\" -1\r\n";
        let after = with_line(
            before,
            &["banid"],
            "76561190000000001",
            Some("banid 76561190000000001 \"A\" \"z\" -1".into()),
        );
        assert_eq!(
            after,
            "banid 76561190000000002 \"B\" \"y\" -1\r\nbanid 76561190000000001 \"A\" \"z\" -1\r\n"
        );
        let removed = with_line(&after, &["banid"], "76561190000000002", None);
        assert_eq!(removed, "banid 76561190000000001 \"A\" \"z\" -1\r\n");
    }

    #[test]
    fn reads_ban_lengths() {
        assert_eq!(duration_seconds("3h"), Some(10_800));
        assert_eq!(duration_seconds("2d"), Some(172_800));
        assert_eq!(duration_seconds("0d"), None);
        assert_eq!(duration_seconds("5w"), None);
        assert_eq!(duration_seconds(""), None);
    }

    #[test]
    fn checks_steam_ids() {
        assert!(is_steam_id("76561198000000001"));
        assert!(!is_steam_id("7656119800000000"));
        assert!(!is_steam_id("1234567890123456a"));
    }

    #[test]
    fn reads_how_the_server_is_coping() {
        let json = r#"{"Hostname": "Rust test", "MaxPlayers": 500, "Players": 0, "Queued": 1, "Joining": 2, "EntityCount": 912, "GameTime": "10/05/2026 09:41:17", "Uptime": 120, "Map": "Procedural Map", "Framerate": 29.5, "Memory": 2471, "NetworkIn": 1200, "NetworkOut": 3400, "Restarting": false}"#;
        let health = parse_health(json).unwrap();
        assert_eq!(health.entities, 912);
        assert_eq!((health.queued, health.joining), (1, 2));
        assert!((health.framerate - 29.5).abs() < f64::EPSILON);
        assert!(parse_health("not json").is_none());
    }

    #[test]
    fn reads_where_rust_plus_listens() {
        let text = "Server ID: 19d1c9cc
Listening on: 0.0.0.0:28083
App connects to: 203.0.113.7:28083";
        assert_eq!(
            parse_app_info(text),
            (
                Some("0.0.0.0:28083".into()),
                Some("203.0.113.7:28083".into())
            )
        );
        assert_eq!(parse_app_info(""), (None, None));
    }

    #[test]
    fn reads_chat_history() {
        let json = r##"[{"Channel":2,"Message":"Hello from the panel","UserId":"0","Username":"SERVER","Color":"#eee","Time":1791166292}]"##;
        let chat = parse_chat(json);
        assert_eq!(chat[0].username, "SERVER");
        assert_eq!(chat[0].time, 1_791_166_292);
    }
}
