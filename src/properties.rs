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
            if let Line::Entry { key: found, value } = line {
                if found == key {
                    *value = escaped;
                    return;
                }
            }
        }
        self.lines.push(Line::Entry {
            key: key.to_string(),
            value: escaped,
        });
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
];

/// What may be set when a server of this kind is made. The panel writes the port
/// and the RCON keys itself, so they are deliberately absent.
pub fn catalogue(kind: &str) -> &'static [Known] {
    match kind {
        "minecraft_bedrock" => BEDROCK,
        _ => JAVA,
    }
}

pub fn known(kind: &str, key: &str) -> Option<&'static Known> {
    catalogue(kind).iter().find(|entry| entry.key == key)
}

/// Checks one value against what the key accepts, and returns it as the file
/// wants it written.
pub fn check(entry: &Known, value: &str) -> std::result::Result<String, String> {
    let value = value.trim();
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
        for kind in ["minecraft_java", "minecraft_bedrock"] {
            for entry in catalogue(kind) {
                if matches!(entry.kind, Kind::Text) && entry.default.is_empty() {
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
