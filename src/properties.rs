use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Serialize;

/// A `server.properties` file held line by line, so writing it back only
/// disturbs the keys that changed. Both Java and Bedrock use this format.
pub struct Properties {
    path: PathBuf,
    lines: Vec<Line>,
    crlf: bool,
}

enum Line {
    /// A comment, a blank line, or anything that is not `key=value`.
    Verbatim(String),
    Entry {
        key: String,
        value: String,
    },
}

impl Properties {
    pub async fn load_if_present(path: impl Into<PathBuf>) -> Result<Option<Self>> {
        let path = path.into();
        match tokio::fs::read_to_string(&path).await {
            Ok(text) => Ok(Some(Self::parse(path, &text))),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error).with_context(|| format!("reading {}", path.display())),
        }
    }

    /// A file that is not there yet. Saving it creates one holding only the keys
    /// that were set; the server fills in the rest the first time it runs.
    pub fn empty(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            lines: Vec::new(),
            crlf: false,
        }
    }

    fn parse(path: PathBuf, text: &str) -> Self {
        let crlf = text.contains("\r\n");
        let lines = text
            .lines()
            .map(|line| {
                let trimmed = line.trim_start();
                if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('!') {
                    return Line::Verbatim(line.to_string());
                }
                match split_entry(line) {
                    Some((key, value)) => Line::Entry { key, value },
                    None => Line::Verbatim(line.to_string()),
                }
            })
            .collect();
        Self { path, lines, crlf }
    }

    /// The value with Java's escapes resolved.
    pub fn get(&self, key: &str) -> Option<String> {
        self.lines.iter().find_map(|line| match line {
            Line::Entry { key: found, value } if found == key => Some(unescape(value)),
            _ => None,
        })
    }

    pub fn flag(&self, key: &str) -> Option<bool> {
        self.get(key).map(|value| value.trim() == "true")
    }

    pub fn number(&self, key: &str) -> Option<i64> {
        self.get(key)?.trim().parse().ok()
    }

    pub fn set(&mut self, key: &str, value: &str) {
        let escaped = escape(value);
        for line in &mut self.lines {
            if let Line::Entry { key: found, value } = line
                && found == key
            {
                *value = escaped;
                return;
            }
        }
        self.lines.push(Line::Entry {
            key: key.to_string(),
            value: escaped,
        });
    }

    /// Every entry in the file, in the order it appears.
    pub fn entries(&self) -> Vec<(String, String)> {
        self.lines
            .iter()
            .filter_map(|line| match line {
                Line::Entry { key, value } => Some((key.clone(), unescape(value))),
                Line::Verbatim(_) => None,
            })
            .collect()
    }

    pub async fn save(&self) -> Result<()> {
        let separator = if self.crlf { "\r\n" } else { "\n" };
        let mut out = String::new();
        for line in &self.lines {
            match line {
                Line::Verbatim(text) => out.push_str(text),
                Line::Entry { key, value } => {
                    out.push_str(&escape_key(key));
                    out.push('=');
                    out.push_str(value);
                }
            }
            out.push_str(separator);
        }

        // Write beside the file and rename, so a crash cannot leave it half written.
        let temporary = self.path.with_extension("properties.tmp");
        tokio::fs::write(&temporary, out.as_bytes())
            .await
            .with_context(|| format!("writing {}", temporary.display()))?;
        tokio::fs::rename(&temporary, &self.path)
            .await
            .with_context(|| format!("replacing {}", self.path.display()))?;
        Ok(())
    }
}

/// Splits on the first unescaped `=` or `:`, the way java.util.Properties does.
fn split_entry(line: &str) -> Option<(String, String)> {
    let mut escaped = false;
    for (index, byte) in line.bytes().enumerate() {
        if escaped {
            escaped = false;
            continue;
        }
        match byte {
            b'\\' => escaped = true,
            b'=' | b':' => {
                let key = unescape(line[..index].trim());
                if key.is_empty() {
                    return None;
                }
                return Some((key, line[index + 1..].trim_start().to_string()));
            }
            _ => {}
        }
    }
    None
}

fn unescape(value: &str) -> String {
    if !value.contains('\\') {
        return value.to_string();
    }
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('u') => {
                let digits: String = chars.by_ref().take(4).collect();
                match u32::from_str_radix(&digits, 16)
                    .ok()
                    .and_then(char::from_u32)
                {
                    Some(decoded) => out.push(decoded),
                    None => {
                        out.push_str("\\u");
                        out.push_str(&digits);
                    }
                }
            }
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

fn escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if (ch as u32) > 0x7e => out.push_str(&format!("\\u{:04x}", ch as u32)),
            ch => out.push(ch),
        }
    }
    out
}

fn escape_key(key: &str) -> String {
    escape(key).replace(':', "\\:").replace('=', "\\=")
}

/// `server.properties` inside a server folder.
pub fn path_in(directory: &Path) -> PathBuf {
    directory.join("server.properties")
}

/// One key the panel knows how to present and check. Only the settings worth
/// choosing when a server is made are here; the rest are edited in the file.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Known {
    pub key: &'static str,
    pub label: &'static str,
    pub help: &'static str,
    pub group: &'static str,
    pub default: &'static str,
    #[serde(flatten)]
    pub kind: Kind,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Kind {
    Text,
    Flag,
    Number { min: i64, max: i64 },
    Decimal { min: f64, max: f64 },
    Choice { options: &'static [&'static str] },
}

const JAVA: &[Known] = &[
    Known {
        key: "level-seed",
        label: "Seed",
        help: "Leave empty for a random world.",
        group: "World",
        default: "",
        kind: Kind::Text,
    },
    Known {
        key: "level-type",
        label: "World type",
        help: "How the world is generated.",
        group: "World",
        default: "minecraft:normal",
        kind: Kind::Choice {
            options: &[
                "minecraft:normal",
                "minecraft:flat",
                "minecraft:large_biomes",
                "minecraft:amplified",
                "minecraft:single_biome_surface",
            ],
        },
    },
    Known {
        key: "difficulty",
        label: "Difficulty",
        help: "",
        group: "World",
        default: "easy",
        kind: Kind::Choice {
            options: &["peaceful", "easy", "normal", "hard"],
        },
    },
    Known {
        key: "hardcore",
        label: "Hardcore",
        help: "Death is permanent and the difficulty is locked to hard.",
        group: "World",
        default: "false",
        kind: Kind::Flag,
    },
    Known {
        key: "initial-enabled-packs",
        label: "Data packs",
        help: "Comma-separated, and only read when the world is first made. \
               Naming a feature pack here is how this version's experiments are turned on.",
        group: "World",
        default: "vanilla",
        kind: Kind::Text,
    },
    Known {
        key: "motd",
        label: "Message of the day",
        help: "The line players see under the server name in their list.",
        group: "Players",
        default: "A Minecraft Server",
        kind: Kind::Text,
    },
    Known {
        key: "max-players",
        label: "Player slots",
        help: "",
        group: "Players",
        default: "20",
        kind: Kind::Number { min: 1, max: 1000 },
    },
    Known {
        key: "gamemode",
        label: "Game mode",
        help: "What players start in.",
        group: "Players",
        default: "survival",
        kind: Kind::Choice {
            options: &["survival", "creative", "adventure", "spectator"],
        },
    },
    Known {
        key: "online-mode",
        label: "Check accounts with Mojang",
        help: "Turn this off only behind a proxy that checks them for you.",
        group: "Players",
        default: "true",
        kind: Kind::Flag,
    },
    Known {
        key: "white-list",
        label: "Allow list only",
        help: "On by default. Nobody can join until they are on the list.",
        group: "Players",
        default: "true",
        kind: Kind::Flag,
    },
    Known {
        key: "view-distance",
        label: "View distance",
        help: "Chunks sent to each player. Lower costs less memory.",
        group: "Server",
        default: "10",
        kind: Kind::Number { min: 3, max: 32 },
    },
    Known {
        key: "simulation-distance",
        label: "Simulation distance",
        help: "Chunks kept ticking around each player.",
        group: "Server",
        default: "10",
        kind: Kind::Number { min: 3, max: 32 },
    },
    Known {
        key: "spawn-protection",
        label: "Spawn protection",
        help: "Radius around spawn only operators may build in. 0 turns it off.",
        group: "Server",
        default: "16",
        kind: Kind::Number { min: 0, max: 256 },
    },
    Known {
        key: "allow-flight",
        label: "Allow flight",
        help: "For survival players with a mod or plugin that grants it.",
        group: "Server",
        default: "false",
        kind: Kind::Flag,
    },
    Known {
        key: "generate-structures",
        label: "Generate structures",
        help: "Villages, temples and the rest.",
        group: "World",
        default: "true",
        kind: Kind::Flag,
    },
    Known {
        key: "max-world-size",
        label: "World radius",
        help: "Blocks from the centre the world may grow to.",
        group: "World",
        default: "29999984",
        kind: Kind::Number {
            min: 1,
            max: 29_999_984,
        },
    },
    Known {
        key: "enforce-whitelist",
        label: "Kick players off the allow list",
        help: "Removes players already on the server when they are taken off the list.",
        group: "Players",
        default: "false",
        kind: Kind::Flag,
    },
    Known {
        key: "player-idle-timeout",
        label: "Kick idle players after",
        help: "Minutes. 0 never kicks them.",
        group: "Players",
        default: "0",
        kind: Kind::Number { min: 0, max: 1440 },
    },
    Known {
        key: "op-permission-level",
        label: "Operator level",
        help: "1 bypasses spawn protection, 4 allows every command.",
        group: "Players",
        default: "4",
        kind: Kind::Number { min: 0, max: 4 },
    },
    Known {
        key: "hide-online-players",
        label: "Hide the player list",
        help: "Keeps names out of the reply the server list gets.",
        group: "Players",
        default: "false",
        kind: Kind::Flag,
    },
    Known {
        key: "enforce-secure-profile",
        label: "Require signed chat",
        help: "Turning this off lets players without a Mojang chat signature join.",
        group: "Players",
        default: "true",
        kind: Kind::Flag,
    },
    Known {
        key: "prevent-proxy-connections",
        label: "Block proxied connections",
        help: "Refuses players whose account region does not match their address.",
        group: "Players",
        default: "false",
        kind: Kind::Flag,
    },
    Known {
        key: "resource-pack",
        label: "Resource pack",
        help: "A URL the client downloads on join.",
        group: "Resource pack",
        default: "",
        kind: Kind::Text,
    },
    Known {
        key: "require-resource-pack",
        label: "Require the resource pack",
        help: "Players who refuse it are kicked.",
        group: "Resource pack",
        default: "false",
        kind: Kind::Flag,
    },
    Known {
        key: "pause-when-empty-seconds",
        label: "Pause when empty after",
        help: "Seconds with nobody on before the server stops ticking. 0 keeps it running.",
        group: "Server",
        default: "60",
        kind: Kind::Number { min: 0, max: 86400 },
    },
    Known {
        key: "network-compression-threshold",
        label: "Compress packets over",
        help: "Bytes. -1 turns compression off.",
        group: "Server",
        default: "256",
        kind: Kind::Number {
            min: -1,
            max: 65536,
        },
    },
    Known {
        key: "entity-broadcast-range-percentage",
        label: "Entity range",
        help: "Percent of the normal distance at which entities are sent to players.",
        group: "Server",
        default: "100",
        kind: Kind::Number { min: 10, max: 1000 },
    },
    Known {
        key: "sync-chunk-writes",
        label: "Write chunks synchronously",
        help: "Safer on a crash, slower on some disks.",
        group: "Server",
        default: "true",
        kind: Kind::Flag,
    },
];

const BEDROCK: &[Known] = &[
    Known {
        key: "level-seed",
        label: "Seed",
        help: "Leave empty for a random world.",
        group: "World",
        default: "",
        kind: Kind::Text,
    },
    Known {
        key: "difficulty",
        label: "Difficulty",
        help: "",
        group: "World",
        default: "easy",
        kind: Kind::Choice {
            options: &["peaceful", "easy", "normal", "hard"],
        },
    },
    Known {
        key: "allow-cheats",
        label: "Allow cheats",
        help: "Lets operators use commands that change the world.",
        group: "World",
        default: "false",
        kind: Kind::Flag,
    },
    Known {
        key: "server-name",
        label: "Server name",
        help: "What players see in their server list.",
        group: "Players",
        default: "Dedicated Server",
        kind: Kind::Text,
    },
    Known {
        key: "max-players",
        label: "Player slots",
        help: "",
        group: "Players",
        default: "10",
        kind: Kind::Number { min: 1, max: 1000 },
    },
    Known {
        key: "gamemode",
        label: "Game mode",
        help: "Bedrock has no spectator mode here.",
        group: "Players",
        default: "survival",
        kind: Kind::Choice {
            options: &["survival", "creative", "adventure"],
        },
    },
    Known {
        key: "online-mode",
        label: "Check Xbox Live accounts",
        help: "",
        group: "Players",
        default: "true",
        kind: Kind::Flag,
    },
    Known {
        key: "allow-list",
        label: "Allow list only",
        help: "On by default. Nobody can join until they are on the list.",
        group: "Players",
        default: "true",
        kind: Kind::Flag,
    },
    Known {
        key: "default-player-permission-level",
        label: "New players join as",
        help: "",
        group: "Players",
        default: "member",
        kind: Kind::Choice {
            options: &["visitor", "member", "operator"],
        },
    },
    Known {
        key: "transport",
        label: "Transport",
        help: "Mojang now ships nethernet, which routes through their signalling \
               service and opens no port. Choose raknet for the classic UDP listener \
               that lets players connect by address and lets the panel read status.",
        group: "Server",
        default: "nethernet",
        kind: Kind::Choice {
            options: &["raknet", "nethernet"],
        },
    },
    Known {
        key: "view-distance",
        label: "View distance",
        help: "Chunks sent to each player. Lower costs less memory.",
        group: "Server",
        default: "32",
        kind: Kind::Number { min: 5, max: 64 },
    },
    Known {
        key: "tick-distance",
        label: "Tick distance",
        help: "Chunks kept ticking around each player.",
        group: "Server",
        default: "4",
        kind: Kind::Number { min: 4, max: 12 },
    },
    Known {
        key: "player-idle-timeout",
        label: "Kick idle players after",
        help: "Minutes. 0 never kicks them.",
        group: "Server",
        default: "30",
        kind: Kind::Number { min: 0, max: 1440 },
    },
    Known {
        key: "force-gamemode",
        label: "Force the game mode",
        help: "Puts returning players back into the server's mode.",
        group: "Players",
        default: "false",
        kind: Kind::Flag,
    },
    Known {
        key: "chat-restriction",
        label: "Chat",
        help: "Dropped silently discards messages; Disabled tells the player.",
        group: "Players",
        default: "None",
        kind: Kind::Choice {
            options: &["None", "Dropped", "Disabled"],
        },
    },
    Known {
        key: "disable-player-interaction",
        label: "Stop players interacting",
        help: "They can still move and look around.",
        group: "Players",
        default: "false",
        kind: Kind::Flag,
    },
    Known {
        key: "disable-custom-skins",
        label: "Block custom skins",
        help: "Players fall back to a standard skin.",
        group: "Players",
        default: "false",
        kind: Kind::Flag,
    },
    Known {
        key: "texturepack-required",
        label: "Require the resource pack",
        help: "Players who refuse the server's pack cannot join.",
        group: "Resource pack",
        default: "false",
        kind: Kind::Flag,
    },
    Known {
        key: "enable-lan-visibility",
        label: "Show on the local network",
        help: "Puts the server in the in-game list for machines on the same network.",
        group: "Server",
        default: "true",
        kind: Kind::Flag,
    },
    Known {
        key: "max-threads",
        label: "Threads",
        help: "0 lets the server use as many as it wants.",
        group: "Server",
        default: "8",
        kind: Kind::Number { min: 0, max: 256 },
    },
    Known {
        key: "client-side-chunk-generation-enabled",
        label: "Let clients build chunks",
        help: "Clients draw terrain ahead of the server sending it.",
        group: "Server",
        default: "true",
        kind: Kind::Flag,
    },
    Known {
        key: "compression-algorithm",
        label: "Compression",
        help: "snappy costs less CPU, zlib less bandwidth.",
        group: "Server",
        default: "zlib",
        kind: Kind::Choice {
            options: &["zlib", "snappy"],
        },
    },
];

/// Rust has no settings file. These are console variables, handed to the server on
/// its command line as `+name value` when it is made.
const RUST: &[Known] = &[
    Known {
        key: "server.level",
        label: "Map",
        help: "Procedural generates a new map from the seed.",
        group: "World",
        default: "Procedural Map",
        kind: Kind::Choice {
            options: &[
                "Procedural Map",
                "Barren",
                "HapisIsland",
                "CraggyIsland",
                "SavasIsland",
            ],
        },
    },
    Known {
        key: "server.levelurl",
        label: "Custom map link",
        help: "A direct download link to a .map file. Leave the seed and map size empty when you use one.",
        group: "World",
        default: "",
        kind: Kind::Text,
    },
    Known {
        key: "server.seed",
        label: "Seed",
        help: "Leave empty for a random map. Only the procedural map uses it.",
        group: "World",
        default: "",
        kind: Kind::Number {
            min: 1,
            max: 2_147_483_647,
        },
    },
    Known {
        key: "server.worldsize",
        label: "Map size",
        help: "Metres along one side. Bigger maps cost a lot more memory and disk.",
        group: "World",
        default: "3000",
        kind: Kind::Number {
            min: 1000,
            max: 6000,
        },
    },
    Known {
        key: "server.saveinterval",
        label: "Save interval",
        help: "Seconds between saves of the world. A crash loses what happened since.",
        group: "World",
        default: "600",
        kind: Kind::Number { min: 60, max: 3600 },
    },
    Known {
        key: "server.maxplayers",
        label: "Player slots",
        help: "",
        group: "Players",
        default: "500",
        kind: Kind::Number { min: 1, max: 1000 },
    },
    Known {
        key: "server.gamemode",
        label: "Game mode",
        help: "Changing this part way through a wipe resets every player and their inventory.",
        group: "Players",
        default: "standard",
        kind: Kind::Choice {
            options: &["standard", "softcore", "hardcore", "primitive"],
        },
    },
    Known {
        key: "server.pve",
        label: "Peaceful (PvE)",
        help: "Players cannot hurt each other.",
        group: "Players",
        default: "false",
        kind: Kind::Flag,
    },
    Known {
        key: "relationshipmanager.maxteamsize",
        label: "Team size",
        help: "Most players allowed in one team.",
        group: "Players",
        default: "8",
        kind: Kind::Number { min: 1, max: 100 },
    },
    Known {
        key: "server.idlekick",
        label: "Kick idle players",
        help: "Minutes without input before a player is removed. 0 never kicks.",
        group: "Players",
        default: "30",
        kind: Kind::Number { min: 0, max: 1440 },
    },
    Known {
        key: "server.hostname",
        label: "Server name",
        help: "What the in-game list shows. Empty uses the name this server has in the panel.",
        group: "Listing",
        default: "",
        kind: Kind::Text,
    },
    Known {
        key: "server.description",
        label: "Description",
        help: "Shown on the server's page in the in-game browser.",
        group: "Listing",
        default: "",
        kind: Kind::Text,
    },
    Known {
        key: "server.url",
        label: "Website",
        help: "A link players can open from the server's page.",
        group: "Listing",
        default: "",
        kind: Kind::Text,
    },
    Known {
        key: "server.headerimage",
        label: "Header image",
        help: "A direct link to a picture for the top of the server's page. 512 by 256 pixels works well.",
        group: "Listing",
        default: "",
        kind: Kind::Text,
    },
    Known {
        key: "server.logoimage",
        label: "Icon",
        help: "A direct link to a 256 by 256 picture. It is shown in a circle, so keep the corners empty.",
        group: "Listing",
        default: "",
        kind: Kind::Text,
    },
    Known {
        key: "server.tags",
        label: "Tags",
        help: "The server browser shows up to four.",
        group: "Listing",
        default: "",
        kind: Kind::Text,
    },
    Known {
        key: "server.tickrate",
        label: "Tick rate",
        help: "Updates per second. Higher is smoother and costs more CPU.",
        group: "Rules",
        default: "10",
        kind: Kind::Number { min: 1, max: 100 },
    },
    Known {
        key: "craft.instant",
        label: "Instant crafting",
        help: "",
        group: "Rules",
        default: "false",
        kind: Kind::Flag,
    },
    Known {
        key: "server.stability",
        label: "Building stability",
        help: "Off lets structures stand without support.",
        group: "Rules",
        default: "true",
        kind: Kind::Flag,
    },
    Known {
        key: "server.radiation",
        label: "Radiation",
        help: "",
        group: "Rules",
        default: "true",
        kind: Kind::Flag,
    },
    Known {
        key: "decay.upkeep",
        label: "Upkeep",
        help: "Whether bases need materials in a tool cupboard to stay safe from decay.",
        group: "Rules",
        default: "true",
        kind: Kind::Flag,
    },
    Known {
        key: "decay.scale",
        label: "Decay speed",
        help: "1 is normal and 0 turns decay off. 2 is twice as fast.",
        group: "Rules",
        default: "1",
        kind: Kind::Decimal {
            min: 0.0,
            max: 100.0,
        },
    },
    Known {
        key: "server.corpsedespawn",
        label: "Corpse lifetime",
        help: "Seconds before a body disappears.",
        group: "Rules",
        default: "300",
        kind: Kind::Number { min: 0, max: 3600 },
    },
    Known {
        key: "server.itemdespawn",
        label: "Dropped item lifetime",
        help: "Seconds before an item left on the ground disappears.",
        group: "Rules",
        default: "300",
        kind: Kind::Number { min: 10, max: 3600 },
    },
    Known {
        key: "bradley.enabled",
        label: "Bradley APC",
        help: "The armoured vehicle that guards the launch site.",
        group: "Events",
        default: "true",
        kind: Kind::Flag,
    },
    Known {
        key: "cargoship.event_enabled",
        label: "Cargo ship",
        help: "",
        group: "Events",
        default: "true",
        kind: Kind::Flag,
    },
    Known {
        key: "ai.npc_enable",
        label: "Scientists and NPCs",
        help: "Off removes the human NPCs at monuments.",
        group: "Events",
        default: "true",
        kind: Kind::Flag,
    },
    Known {
        key: "wipetimer.wipedayofweek",
        label: "Wipe day",
        help: "0 is Sunday and 6 is Saturday. Drives the countdown in game, and the end-of-wipe events, together with a wipe tag.",
        group: "Wipe",
        default: "4",
        kind: Kind::Number { min: 0, max: 6 },
    },
    Known {
        key: "wipetimer.wipehourofday",
        label: "Wipe hour",
        help: "24-hour clock. 14.5 is half past two in the afternoon.",
        group: "Wipe",
        default: "19",
        kind: Kind::Decimal {
            min: 0.0,
            max: 23.99,
        },
    },
    Known {
        key: "wipetimer.wipetimezone",
        label: "Wipe time zone",
        help: "A time zone name such as Europe/London or America/New_York.",
        group: "Wipe",
        default: "Europe/London",
        kind: Kind::Text,
    },
    Known {
        key: "wipetimer.wipecronoverride",
        label: "Wipe schedule",
        help: "A cron line that replaces the day and hour, such as 0 19 1-7 * 4 for the first Thursday at 7pm.",
        group: "Wipe",
        default: "",
        kind: Kind::Text,
    },
    Known {
        key: "server.tutorialenabled",
        label: "Tutorial island",
        help: "Starts new players on a practice island. Add the tut tag so the browser says so.",
        group: "Players",
        default: "false",
        kind: Kind::Flag,
    },
    Known {
        key: "server.censorplayerlist",
        label: "Hide player names",
        help: "Keeps who is on out of the server browser.",
        group: "Players",
        default: "true",
        kind: Kind::Flag,
    },
    Known {
        key: "server.printreportstoconsole",
        label: "F7 reports in the console",
        help: "Prints what players report with F7, so it shows in the console here.",
        group: "Players",
        default: "false",
        kind: Kind::Flag,
    },
    Known {
        key: "server.savebackupcount",
        label: "Save backups kept",
        help: "Earlier saves Rust keeps beside the current one.",
        group: "World",
        default: "2",
        kind: Kind::Number { min: 0, max: 20 },
    },
    Known {
        key: "boombox.serverurllist",
        label: "Radio stations",
        help: "Name,URL pairs for boomboxes, comma-separated. Each URL has to be a direct audio stream.",
        group: "Listing",
        default: "",
        kind: Kind::Text,
    },
    Known {
        key: "server.favoritesendpoint",
        label: "Favourites address",
        help: "A host name with an SRV record for the game port, so favourites survive an IP change.",
        group: "Listing",
        default: "",
        kind: Kind::Text,
    },
    Known {
        key: "creative.allusers",
        label: "Creative for everyone",
        help: "Every player builds and crafts for free.",
        group: "Creative",
        default: "false",
        kind: Kind::Flag,
    },
    Known {
        key: "creative.freebuild",
        label: "Free building",
        help: "",
        group: "Creative",
        default: "false",
        kind: Kind::Flag,
    },
    Known {
        key: "creative.freeplacement",
        label: "Place anywhere",
        help: "Ignores the rules about where things can go.",
        group: "Creative",
        default: "false",
        kind: Kind::Flag,
    },
    Known {
        key: "creative.freerepair",
        label: "Free repairs",
        help: "",
        group: "Creative",
        default: "false",
        kind: Kind::Flag,
    },
    Known {
        key: "creative.unlimitedio",
        label: "Unlimited electricity",
        help: "Electrical and industrial parts run without power.",
        group: "Creative",
        default: "false",
        kind: Kind::Flag,
    },
    Known {
        key: "app.port",
        label: "Rust+ port",
        help: "The TCP port the Rust+ app reaches. Empty picks the game port plus 68, -1 turns Rust+ off, and Docker has to publish it.",
        group: "Rust+",
        default: "",
        kind: Kind::Number {
            min: -1,
            max: 65535,
        },
    },
    Known {
        key: "app.publicip",
        label: "Rust+ public address",
        help: "The address phones reach the server on, for a server behind Docker or a router.",
        group: "Rust+",
        default: "",
        kind: Kind::Text,
    },
];

/// What may be set when a server of this kind is made. The panel writes the port
/// and the RCON keys itself, so they are deliberately absent.
pub fn catalogue(kind: &str) -> &'static [Known] {
    match kind {
        "minecraft_bedrock" => BEDROCK,
        crate::rust_game::KIND => RUST,
        _ => JAVA,
    }
}

pub fn known(kind: &str, key: &str) -> Option<&'static Known> {
    catalogue(kind).iter().find(|entry| entry.key == key)
}

/// Keys the panel writes itself. Editing them from outside would put the server
/// row and the file out of step, or hand back the RCON password.
pub const RESERVED: [&str; 5] = [
    "server-port",
    "server-portv6",
    "enable-rcon",
    "rcon.port",
    "rcon.password",
];

pub fn is_reserved(key: &str) -> bool {
    RESERVED.contains(&key)
}

/// A key the catalogue says nothing about. The file format is the only rule
/// left, so hold it to that and no more.
pub fn check_raw(key: &str, value: &str) -> std::result::Result<String, String> {
    if key.trim().is_empty() {
        return Err("Give the setting a name.".into());
    }
    if key.chars().any(|ch| ch.is_whitespace() || ch.is_control()) {
        return Err("A name cannot hold spaces or control characters.".into());
    }
    if value.chars().any(|ch| ch == '\n' || ch == '\r') {
        return Err("A value has to stay on one line.".into());
    }
    if value.chars().count() > 1024 {
        return Err("That is too long.".into());
    }
    Ok(value.to_string())
}

/// Checks one value against what the key accepts, and returns it as the file
/// wants it written.
pub fn check(entry: &Known, value: &str) -> std::result::Result<String, String> {
    let value = value.trim();
    if entry.key == "server.tags" {
        return crate::rust_game::check_tags(value);
    }
    match entry.kind {
        Kind::Flag => match value {
            "true" | "false" => Ok(value.to_string()),
            _ => Err("Use true or false.".into()),
        },
        Kind::Number { min, max } => match value.parse::<i64>() {
            Ok(number) if (min..=max).contains(&number) => Ok(number.to_string()),
            Ok(_) => Err(format!("Choose a number between {min} and {max}.")),
            Err(_) => Err("This has to be a whole number.".into()),
        },
        Kind::Decimal { min, max } => match value.parse::<f64>() {
            Ok(number) if number.is_finite() && (min..=max).contains(&number) => {
                Ok(number.to_string())
            }
            Ok(_) => Err(format!("Choose a number between {min} and {max}.")),
            Err(_) => Err("This has to be a number.".into()),
        },
        Kind::Choice { options } => match options.contains(&value) {
            true => Ok(value.to_string()),
            false => Err(format!("Choose one of: {}.", options.join(", "))),
        },
        // The writer escapes anything awkward, so only length is worth refusing.
        Kind::Text if value.chars().count() > 512 => Err("That is too long.".into()),
        Kind::Text => Ok(value.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rendered(properties: &Properties) -> Vec<String> {
        properties
            .lines
            .iter()
            .map(|line| match line {
                Line::Verbatim(text) => text.clone(),
                Line::Entry { key, value } => format!("{key}={value}"),
            })
            .collect()
    }

    #[test]
    fn keeps_comments_and_changes_one_key() {
        let mut properties = Properties::parse(
            PathBuf::from("server.properties"),
            "#Minecraft server properties\nmotd=A Minecraft Server\nenable-rcon=false\n",
        );
        assert_eq!(
            properties.get("motd").as_deref(),
            Some("A Minecraft Server")
        );
        assert_eq!(properties.flag("enable-rcon"), Some(false));

        properties.set("enable-rcon", "true");
        properties.set("rcon.password", "hunter2");

        assert_eq!(
            rendered(&properties),
            vec![
                "#Minecraft server properties",
                "motd=A Minecraft Server",
                "enable-rcon=true",
                "rcon.password=hunter2",
            ]
        );
    }

    #[test]
    fn resolves_java_escapes() {
        let properties = Properties::parse(
            PathBuf::from("server.properties"),
            "motd=Caf\\u00e9 \\u00a7aserver\nlevel-name=a\\:b\n",
        );
        assert_eq!(properties.get("motd").as_deref(), Some("Café §aserver"));
        assert_eq!(properties.get("level-name").as_deref(), Some("a:b"));
    }

    #[test]
    fn checks_a_value_against_what_the_key_accepts() {
        let difficulty = known("minecraft_java", "difficulty").expect("known");
        assert_eq!(check(difficulty, "hard").as_deref(), Ok("hard"));
        assert!(check(difficulty, "brutal").is_err());

        let slots = known("minecraft_java", "max-players").expect("known");
        assert_eq!(check(slots, " 40 ").as_deref(), Ok("40"));
        assert!(check(slots, "0").is_err());
        assert!(check(slots, "lots").is_err());

        let hardcore = known("minecraft_java", "hardcore").expect("known");
        assert_eq!(check(hardcore, "true").as_deref(), Ok("true"));
        assert!(check(hardcore, "yes").is_err());
    }

    /// The panel owns the port and the RCON credentials; letting a request set
    /// them would put the row and the file out of step.
    #[test]
    fn the_keys_the_panel_manages_are_not_on_offer() {
        for kind in ["minecraft_java", "minecraft_bedrock"] {
            for key in [
                "server-port",
                "server-portv6",
                "rcon.password",
                "rcon.port",
                "enable-rcon",
                "level-name",
            ] {
                assert!(known(kind, key).is_none(), "{kind} should not offer {key}");
            }
        }
    }

    /// Bedrock names several of these differently, so a Java key must not slip in.
    #[test]
    fn each_kind_offers_only_its_own_keys() {
        assert!(known("minecraft_java", "motd").is_some());
        assert!(known("minecraft_bedrock", "motd").is_none());
        assert!(known("minecraft_bedrock", "server-name").is_some());

        assert!(known("minecraft_java", "white-list").is_some());
        assert!(known("minecraft_bedrock", "allow-list").is_some());
        assert!(known("minecraft_bedrock", "white-list").is_none());

        // Bedrock has no spectator mode.
        let bedrock = known("minecraft_bedrock", "gamemode").expect("known");
        assert!(check(bedrock, "spectator").is_err());
        assert!(check(bedrock, "adventure").is_ok());

        // Java turns experiments on with a feature pack; Bedrock keeps its
        // toggles in the world file, out of reach of server.properties.
        assert!(known("minecraft_java", "initial-enabled-packs").is_some());
        assert!(known("minecraft_bedrock", "initial-enabled-packs").is_none());

        // Only Bedrock chooses a transport, and only raknet opens a port.
        let transport = known("minecraft_bedrock", "transport").expect("known");
        assert!(check(transport, "raknet").is_ok());
        assert!(check(transport, "tcp").is_err());
        assert!(known("minecraft_java", "transport").is_none());
    }

    /// Both editions ship with the list on, which is the opposite of what people
    /// assume, so the form has to show it that way or the first join fails.
    #[test]
    fn the_allow_list_starts_on_in_both_editions() {
        assert_eq!(
            known("minecraft_java", "white-list").unwrap().default,
            "true"
        );
        assert_eq!(
            known("minecraft_bedrock", "allow-list").unwrap().default,
            "true"
        );
    }

    /// Every default has to satisfy the rule the panel enforces for that key.
    #[test]
    fn every_default_passes_its_own_check() {
        for kind in [
            "minecraft_java",
            "minecraft_bedrock",
            crate::rust_game::KIND,
        ] {
            for entry in catalogue(kind) {
                // Left empty, these are simply not set.
                if entry.default.is_empty() {
                    continue;
                }
                assert!(
                    check(entry, entry.default).is_ok(),
                    "{kind}: {} default {:?} fails its own check",
                    entry.key,
                    entry.default
                );
            }
        }
    }

    #[test]
    fn reads_a_port_and_ignores_junk_lines() {
        let properties = Properties::parse(
            PathBuf::from("server.properties"),
            "rcon.port=25575\nnot an entry\n\nquery.port = 25565\n",
        );
        assert_eq!(properties.number("rcon.port"), Some(25575));
        assert_eq!(properties.number("query.port"), Some(25565));
        assert_eq!(properties.get("not an entry"), None);
    }
}
