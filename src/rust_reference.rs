//! Rust's convars, commands and items: Carbon's lists, or a running server's own `find .`.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::Value;
use tokio::sync::Mutex;

const CONVARS_URL: &str = "https://api.carbonmod.gg/meta/rust/convars.json";
const COMMANDS_URL: &str = "https://api.carbonmod.gg/meta/rust/commands.json";
const ITEMS_URL: &str = "https://api.carbonmod.gg/meta/rust/items.json";
pub const ICONS_URL: &str = "https://cdn.carbonmod.gg/items";

/// Rust updates monthly, so a day old is fresh enough.
const FRESH_FOR: Duration = Duration::from_secs(24 * 60 * 60);
/// After a failed refresh the old list is kept, and asked for again this much later.
const RETRY_AFTER: Duration = Duration::from_secs(10 * 60);

/// Rust marks help it wrote with a tool rather than by hand.
const GENERATED: &str = "(Generated)";

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Convar {
    pub name: String,
    pub help: String,
    pub generated: bool,
    /// `bool`, `int`, `float`, `string` or `vector`.
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub default: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Command {
    pub name: String,
    pub help: String,
    pub generated: bool,
}

type Shelf<T> = Mutex<Option<(Instant, Arc<Vec<T>>)>>;

static CONVARS: Shelf<Convar> = Mutex::const_new(None);
static COMMANDS: Shelf<Command> = Mutex::const_new(None);
static ITEMS: Shelf<crate::items::Item> = Mutex::const_new(None);

pub async fn convars(http: &reqwest::Client) -> Result<Arc<Vec<Convar>>, String> {
    shelved(&CONVARS, http, CONVARS_URL, read_convars).await
}

pub async fn commands(http: &reqwest::Client) -> Result<Arc<Vec<Command>>, String> {
    shelved(&COMMANDS, http, COMMANDS_URL, read_commands).await
}

pub async fn items(http: &reqwest::Client) -> Result<Arc<Vec<crate::items::Item>>, String> {
    shelved(&ITEMS, http, ITEMS_URL, read_items).await
}

/// One fetch at a time per list: the lock is held across it, so a page opening
/// three views at once asks Carbon once.
async fn shelved<T>(
    shelf: &Shelf<T>,
    http: &reqwest::Client,
    url: &str,
    read: fn(&Value) -> Vec<T>,
) -> Result<Arc<Vec<T>>, String> {
    let mut held = shelf.lock().await;
    if let Some((at, list)) = held.as_ref()
        && at.elapsed() < FRESH_FOR
    {
        return Ok(list.clone());
    }
    match fetch(http, url).await {
        Ok(value) => {
            let list = Arc::new(read(&value));
            *held = Some((Instant::now(), list.clone()));
            Ok(list)
        }
        Err(error) => match held.as_mut() {
            Some((at, list)) => {
                *at = Instant::now() - FRESH_FOR + RETRY_AFTER;
                Ok(list.clone())
            }
            None => Err(error),
        },
    }
}

async fn fetch(http: &reqwest::Client, url: &str) -> Result<Value, String> {
    http.get(url)
        .timeout(Duration::from_secs(20))
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|error| format!("Carbon's reference did not answer: {error}"))?
        .json()
        .await
        .map_err(|error| format!("Carbon's reference sent something unreadable: {error}"))
}

/// Help without the marker, and whether it had one.
pub fn clean_help(help: &str) -> (String, bool) {
    let help = help.trim();
    match help.strip_prefix(GENERATED) {
        Some(rest) => (rest.trim().to_string(), true),
        None => (help.to_string(), false),
    }
}

fn read_convars(value: &Value) -> Vec<Convar> {
    let mut list: Vec<Convar> = value
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|one| {
            let name = one["Name"].as_str()?.to_string();
            let (help, generated) = clean_help(one["Help"].as_str().unwrap_or_default());
            let kind = match one["Type"].as_str().unwrap_or_default() {
                "bool" => "bool",
                "int" | "long" => "int",
                "float" => "float",
                "UnityEngine.Vector3" => "vector",
                _ => "string",
            };
            let default = match &one["DefaultValue"] {
                Value::Null => None,
                Value::Bool(flag) => Some(flag.to_string()),
                Value::Number(_) => Some(number(&one["DefaultValue"])),
                Value::String(text) => Some(text.clone()),
                Value::Object(vector) => Some(format!(
                    "{},{},{}",
                    number(&vector["x"]),
                    number(&vector["y"]),
                    number(&vector["z"]),
                )),
                Value::Array(_) => None,
            };
            Some(Convar {
                name,
                help,
                generated,
                kind,
                default,
            })
        })
        .collect();
    list.sort_by(|a, b| a.name.cmp(&b.name));
    list
}

/// Carbon writes a float default of ten as `10.0`, which Rust itself prints as `10`.
fn number(value: &Value) -> String {
    match (value.as_i64(), value.as_f64()) {
        (Some(whole), _) => whole.to_string(),
        (None, Some(float)) => float.to_string(),
        _ => String::from("0"),
    }
}

fn read_commands(value: &Value) -> Vec<Command> {
    let mut list: Vec<Command> = value
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter(|one| one["Server"].as_bool().unwrap_or(true))
        .filter_map(|one| {
            let name = one["Name"].as_str()?.to_string();
            let (help, generated) = clean_help(one["Help"].as_str().unwrap_or_default());
            Some(Command {
                name,
                help,
                generated,
            })
        })
        .collect();
    list.sort_by(|a, b| a.name.cmp(&b.name));
    list
}

/// Rust's own item categories, in the order of its `ItemCategory` enum.
const CATEGORIES: [&str; 18] = [
    "weapon",
    "construction",
    "items",
    "resources",
    "attire",
    "tool",
    "medical",
    "food",
    "ammunition",
    "traps",
    "misc",
    "all",
    "common",
    "component",
    "search",
    "favourite",
    "electrical",
    "fun",
];

/// A short name reaches a file path and a url, so it is held to what Rust uses.
pub fn is_short_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && !name.starts_with(['.', ' '])
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | ' '))
}

fn read_items(value: &Value) -> Vec<crate::items::Item> {
    let mut list: Vec<crate::items::Item> = value
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter(|one| !one["Hidden"].as_bool().unwrap_or(false))
        .filter_map(|one| {
            let id = one["ShortName"].as_str()?;
            if !is_short_name(id) {
                return None;
            }
            let category = one["Category"]
                .as_u64()
                .and_then(|at| CATEGORIES.get(at as usize))
                .copied()
                .unwrap_or("misc");
            Some(crate::items::Item {
                id: id.to_string(),
                name: one["DisplayName"].as_str().unwrap_or(id).to_string(),
                category: category.to_string(),
                icon: Some("rust".to_string()),
            })
        })
        .collect();
    list.sort_by(|a, b| a.id.cmp(&b.id));
    list
}

/// What `find .` printed: every convar with its value, then every command.
#[derive(Debug, Default, PartialEq)]
pub struct Listed {
    pub convars: BTreeMap<String, (String, String)>,
    pub commands: BTreeMap<String, String>,
}

/// Reads ` name  help (value)` lines under `Variables:` and ` name(  ) help` under `Commands:`.
pub fn parse_find(text: &str) -> Listed {
    let mut listed = Listed::default();
    let mut in_commands = false;
    for line in text.lines() {
        match line.trim() {
            "Variables:" => in_commands = false,
            "Commands:" => in_commands = true,
            "" => {}
            trimmed if in_commands => {
                let Some((name, rest)) = trimmed.split_once('(') else {
                    continue;
                };
                let help = rest.split_once(')').map_or("", |(_, help)| help);
                listed
                    .commands
                    .insert(name.trim().to_string(), help.trim().to_string());
            }
            trimmed => {
                let (name, rest) = trimmed
                    .split_once(char::is_whitespace)
                    .unwrap_or((trimmed, ""));
                let (help, value) = split_value(rest);
                listed.convars.insert(
                    name.to_string(),
                    (help.trim().to_string(), value.to_string()),
                );
            }
        }
    }
    listed
}

/// The value is the bracketed tail, which may hold brackets of its own.
fn split_value(rest: &str) -> (&str, &str) {
    let rest = rest.trim_end();
    if !rest.ends_with(')') {
        return (rest, "");
    }
    let mut depth = 0usize;
    for (at, ch) in rest.char_indices().rev() {
        match ch {
            ')' => depth += 1,
            '(' => {
                depth -= 1;
                if depth == 0 {
                    return (&rest[..at], &rest[at + 1..rest.len() - 1]);
                }
            }
            _ => {}
        }
    }
    (rest, "")
}

/// What Rust says after a convar is read or set: `name: "value"`.
pub fn parse_reply(name: &str, reply: &str) -> Option<String> {
    let rest = reply.trim().strip_prefix(name)?.strip_prefix(':')?.trim();
    Some(
        rest.strip_prefix('"')
            .and_then(|inner| inner.strip_suffix('"'))
            .unwrap_or(rest)
            .to_string(),
    )
}

/// What `find .` cannot say about a convar Carbon has never listed.
pub fn guess_kind(value: &str) -> &'static str {
    if value.eq_ignore_ascii_case("true") || value.eq_ignore_ascii_case("false") {
        "bool"
    } else if value.parse::<i64>().is_ok() {
        "int"
    } else if value.parse::<f64>().is_ok() {
        "float"
    } else {
        "string"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_output_splits_into_convars_and_commands() {
        let text = "Variables:\n adventcalendar.overrideadventcalendarday  (0)\n server.pve               (Generated) Enables PvE mode (False)\n server.description       Shown in the browser (Hello (EU) test)\n server.tags              Comma-separated tags ()\n\nCommands:\n global.say(  ) Sends a message in chat\n oxide.plugins(  ) \n";
        let listed = parse_find(text);
        assert_eq!(
            listed.convars["adventcalendar.overrideadventcalendarday"],
            (String::new(), "0".to_string())
        );
        assert_eq!(
            listed.convars["server.pve"],
            (
                "(Generated) Enables PvE mode".to_string(),
                "False".to_string()
            )
        );
        assert_eq!(listed.convars["server.description"].1, "Hello (EU) test");
        assert_eq!(listed.convars["server.tags"].1, "");
        assert_eq!(listed.commands["global.say"], "Sends a message in chat");
        assert_eq!(listed.commands["oxide.plugins"], "");
        assert_eq!(listed.convars.len(), 4);
    }

    #[test]
    fn generated_help_loses_its_marker() {
        assert_eq!(
            clean_help(" (Generated) Kills all animals "),
            ("Kills all animals".to_string(), true)
        );
        assert_eq!(
            clean_help("Sends a message in chat"),
            ("Sends a message in chat".to_string(), false)
        );
    }

    #[test]
    fn a_reply_gives_back_the_value() {
        assert_eq!(
            parse_reply("server.pve", "server.pve: \"False\""),
            Some("False".into())
        );
        assert_eq!(
            parse_reply("decay.scale", "decay.scale: \"0.5\"\n"),
            Some("0.5".into())
        );
        assert_eq!(parse_reply("server.pve", "Command not found"), None);
    }

    #[test]
    fn carbon_lists_are_trimmed_to_what_the_panel_shows() {
        let convars = read_convars(&serde_json::json!([
            {"Name": "server.pve", "Help": "(Generated) PvE", "Type": "bool", "DefaultValue": false},
            {"Name": "tutorialisland.overridetutoriallocation", "Help": null, "Type": "UnityEngine.Vector3", "DefaultValue": {"x": 0, "y": 1.5, "z": 0, "magnitude": 1.5}},
            {"Name": "decay.scale", "Help": "", "Type": "float", "DefaultValue": 10.0}
        ]));
        assert_eq!(convars[0].name, "decay.scale");
        assert_eq!(convars[0].default.as_deref(), Some("10"));
        assert_eq!(convars[1].kind, "bool");
        assert_eq!(convars[1].default.as_deref(), Some("false"));
        assert!(convars[1].generated);
        assert_eq!(convars[2].kind, "vector");
        assert_eq!(convars[2].default.as_deref(), Some("0,1.5,0"));

        let items = read_items(&serde_json::json!([
            {"ShortName": "rifle.ak", "DisplayName": "Assault Rifle", "Category": 0, "Hidden": false},
            {"ShortName": "mini fridge", "DisplayName": "Mini Fridge", "Category": 2, "Hidden": false},
            {"ShortName": "debug.thing", "DisplayName": "Debug", "Category": 10, "Hidden": true},
            {"ShortName": "../escape", "DisplayName": "Bad", "Category": 10, "Hidden": false}
        ]));
        let ids: Vec<&str> = items.iter().map(|one| one.id.as_str()).collect();
        assert_eq!(ids, ["mini fridge", "rifle.ak"]);
        assert_eq!(items[1].category, "weapon");
    }

    #[test]
    fn a_kind_is_guessed_from_a_live_value() {
        assert_eq!(guess_kind("True"), "bool");
        assert_eq!(guess_kind("25"), "int");
        assert_eq!(guess_kind("0.5"), "float");
        assert_eq!(guess_kind("Procedural Map"), "string");
    }
}
