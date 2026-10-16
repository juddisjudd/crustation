//! Keen's Enshrouded: a Windows-only server, configured by the one JSON file beside it.

use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::properties::{Kind, Known};

pub const KIND: &str = "enshrouded";
pub const APP_ID: u32 = 2278520;
/// The query port, which since Update 2 carries the game traffic as well.
pub const DEFAULT_PORT: i64 = 15637;
pub const CONFIG_FILE: &str = "enshrouded_server.json";
pub const MAX_SLOTS: i64 = 16;

pub fn executable() -> &'static str {
    "enshrouded_server.exe"
}

pub fn config_path(directory: &Path) -> PathBuf {
    directory.join(CONFIG_FILE)
}

/// Under Wine the server runs from its own folder, which is where it looks for its settings.
pub fn start_command(directory: &Path, wine_prefix: &Path) -> String {
    if cfg!(windows) {
        return shell_words::quote(&directory.join(executable()).to_string_lossy()).into_owned();
    }
    let mut parts = vec!["env".to_string()];
    parts.extend(
        crate::wine::environment(wine_prefix)
            .into_iter()
            .map(|(key, value)| format!("{key}={value}")),
    );
    parts.push("wine".into());
    parts.push(executable().into());
    shell_words::join(parts)
}

pub async fn load(directory: &Path) -> Result<Option<Value>> {
    let path = config_path(directory);
    match tokio::fs::read_to_string(&path).await {
        Ok(text) => {
            let text = text.trim_start_matches('\u{feff}');
            let parsed = serde_json::from_str(text)
                .with_context(|| format!("{} is not valid JSON", path.display()))?;
            Ok(Some(parsed))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| format!("reading {}", path.display())),
    }
}

pub async fn save(directory: &Path, config: &Value) -> Result<()> {
    let path = config_path(directory);
    let temporary = path.with_extension("json.tmp");
    let mut text = serde_json::to_string_pretty(config)?;
    text.push('\n');
    tokio::fs::write(&temporary, text)
        .await
        .with_context(|| format!("writing {}", temporary.display()))?;
    tokio::fs::rename(&temporary, &path)
        .await
        .with_context(|| format!("replacing {}", path.display()))
}

/// The file the server would write on its first start, with the panel choosing the role passwords.
pub fn fresh() -> Value {
    let group =
        |name: &str, kick: bool, inventories: bool, world: bool, base: bool, extend: bool| {
            json!({
                "name": name,
                "password": password(),
                "canKickBan": kick,
                "canAccessInventories": inventories,
                "canEditWorld": world,
                "canEditBase": base,
                "canExtendBase": extend,
                "reservedSlots": 0,
            })
        };
    json!({
        "name": "Enshrouded Server",
        "saveDirectory": "./savegame",
        "logDirectory": "./logs",
        "ip": "0.0.0.0",
        "queryPort": DEFAULT_PORT,
        "slotCount": MAX_SLOTS,
        "tags": [],
        "voiceChatMode": "Proximity",
        "enableVoiceChat": false,
        "enableTextChat": false,
        "gameSettingsPreset": "Default",
        "userGroups": [
            group("Admin", true, true, true, true, true),
            group("Friend", false, true, true, true, false),
            group("Guest", false, false, true, false, false),
            group("Visitor", false, false, false, false, false),
        ],
        "bannedAccounts": [],
    })
}

/// Short enough to type into the game's join box.
pub fn password() -> String {
    crate::rcon::generate_password().chars().take(12).collect()
}

/// Writes the panel's port, address and name into the file, then whatever was asked for at creation.
pub async fn configure(
    directory: &Path,
    name: &str,
    port: i64,
    host: &str,
    settings: &[(String, String)],
) -> Result<()> {
    let mut config = load(directory).await?.unwrap_or_else(fresh);
    set_path(&mut config, "name", json!(name));
    set_path(&mut config, "queryPort", json!(port));
    set_path(&mut config, "ip", json!(listen_address(host)));
    apply(&mut config, settings);
    save(directory, &config).await
}

/// The server takes an IPv4 address or every interface.
pub fn listen_address(host: &str) -> String {
    match host.trim().parse::<Ipv4Addr>() {
        Ok(address) => address.to_string(),
        Err(_) => "0.0.0.0".into(),
    }
}

/// Sets checked values; a game setting moves the preset to Custom, the only one that reads them.
pub fn apply(config: &mut Value, settings: &[(String, String)]) {
    for (key, value) in settings {
        set(config, key, value);
    }
    let custom = settings
        .iter()
        .any(|(key, _)| key.starts_with("gameSettings."));
    let preset_chosen = settings.iter().any(|(key, _)| key == PRESET);
    if custom && !preset_chosen {
        set_path(config, PRESET, json!("Custom"));
    }
}

const PRESET: &str = "gameSettingsPreset";
const TAGS: &str = "tags";

/// Kept in nanoseconds in the file, and offered in minutes.
const DURATIONS: [&str; 3] = [
    "gameSettings.dayTimeDuration",
    "gameSettings.nightTimeDuration",
    "gameSettings.fromHungerToStarving",
];
const NANOS_PER_MINUTE: i64 = 60_000_000_000;

/// The keys the panel owns: the port follows the server row.
pub const MANAGED: [&str; 1] = ["queryPort"];

/// Puts one value in the file as the game types it; empty takes the key out for the game's default.
pub fn set(config: &mut Value, key: &str, value: &str) {
    if value.trim().is_empty() {
        remove_path(config, key);
        return;
    }
    let typed = match CATALOGUE.iter().find(|entry| entry.key == key) {
        Some(entry) => typed(entry, value),
        // Not ours to know, so it keeps the type it already had.
        None => match get_path(config, key) {
            Some(Value::Bool(_)) => json!(value == "true"),
            Some(Value::Number(_)) => number(value).unwrap_or_else(|| json!(value)),
            _ => json!(value),
        },
    };
    set_path(config, key, typed);
}

fn typed(entry: &Known, value: &str) -> Value {
    if entry.key == TAGS {
        let tags: Vec<&str> = value
            .split(',')
            .map(str::trim)
            .filter(|tag| !tag.is_empty())
            .collect();
        return json!(tags);
    }
    match entry.kind {
        Kind::Flag => json!(value == "true"),
        Kind::Number { .. } => {
            let whole = value.parse::<i64>().unwrap_or_default();
            match DURATIONS.contains(&entry.key) {
                true => json!(whole * NANOS_PER_MINUTE),
                false => json!(whole),
            }
        }
        Kind::Decimal { .. } => number(value).unwrap_or_else(|| json!(1)),
        Kind::Text | Kind::Choice { .. } => json!(value),
    }
}

fn number(value: &str) -> Option<Value> {
    if let Ok(whole) = value.trim().parse::<i64>() {
        return Some(json!(whole));
    }
    let decimal = value.trim().parse::<f64>().ok().filter(|n| n.is_finite())?;
    serde_json::Number::from_f64(decimal).map(Value::Number)
}

/// A value as the settings screen shows it.
pub fn shown(config: &Value, key: &str) -> Option<String> {
    let found = get_path(config, key)?;
    let text = match found {
        Value::Bool(flag) => flag.to_string(),
        Value::Number(number) if DURATIONS.contains(&key) => {
            let nanos = number.as_f64().unwrap_or_default();
            ((nanos / NANOS_PER_MINUTE as f64).round() as i64).to_string()
        }
        Value::Number(number) => number.to_string(),
        Value::String(text) => text.clone(),
        Value::Array(items) => items
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(","),
        Value::Null | Value::Object(_) => return None,
    };
    Some(text)
}

/// Every plain value in the file the catalogue does not cover, as `key` or `gameSettings.key`.
pub fn others(config: &Value) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let Some(top) = config.as_object() else {
        return out;
    };
    let mut visit = |key: String, value: &Value| {
        let scalar = matches!(value, Value::Bool(_) | Value::Number(_) | Value::String(_));
        if scalar && !CATALOGUE.iter().any(|entry| entry.key == key) {
            out.push((key.clone(), shown(config, &key).unwrap_or_default()));
        }
    };
    for (key, value) in top {
        visit(key.clone(), value);
    }
    if let Some(Value::Object(game)) = top.get("gameSettings") {
        for (key, value) in game {
            visit(format!("gameSettings.{key}"), value);
        }
    }
    out
}

fn get_path<'a>(config: &'a Value, key: &str) -> Option<&'a Value> {
    key.split('.').try_fold(config, |at, part| at.get(part))
}

fn set_path(config: &mut Value, key: &str, value: Value) {
    let mut at = config;
    let mut parts = key.split('.').peekable();
    while let Some(part) = parts.next() {
        if !at.is_object() {
            *at = Value::Object(Map::new());
        }
        let object = at.as_object_mut().expect("made an object above");
        if parts.peek().is_none() {
            object.insert(part.to_string(), value);
            return;
        }
        at = object
            .entry(part.to_string())
            .or_insert_with(|| Value::Object(Map::new()));
    }
}

fn remove_path(config: &mut Value, key: &str) {
    let (parent, last) = match key.rsplit_once('.') {
        Some((parent, last)) => (
            parent
                .split('.')
                .try_fold(&mut *config, |at, part| at.get_mut(part)),
            last,
        ),
        None => (Some(config), key),
    };
    if let Some(Value::Object(object)) = parent {
        object.remove(last);
    }
}

/// A role players join under by typing its password.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    pub name: String,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub can_kick_ban: bool,
    #[serde(default)]
    pub can_access_inventories: bool,
    #[serde(default)]
    pub can_edit_world: bool,
    #[serde(default)]
    pub can_edit_base: bool,
    #[serde(default)]
    pub can_extend_base: bool,
    #[serde(default)]
    pub reserved_slots: i64,
    /// Whatever a later version adds, carried through untouched.
    #[serde(flatten)]
    pub other: Map<String, Value>,
}

pub fn groups(config: &Value) -> Vec<Group> {
    config
        .get("userGroups")
        .and_then(|groups| serde_json::from_value(groups.clone()).ok())
        .unwrap_or_default()
}

/// The server tells roles apart by password, so names and passwords have to be unique.
pub fn check_groups(groups: &[Group], slots: i64) -> Result<(), String> {
    let mut names: Vec<String> = Vec::new();
    let mut passwords: Vec<&str> = Vec::new();
    let mut reserved = 0;
    for group in groups {
        let name = group.name.trim();
        if name.is_empty() {
            return Err("Give every role a name.".into());
        }
        if name.chars().count() > 32 || name.chars().any(char::is_control) {
            return Err(format!("'{name}' is too long a name for a role."));
        }
        if names.contains(&name.to_lowercase()) {
            return Err(format!("Two roles are called '{name}'."));
        }
        names.push(name.to_lowercase());
        if group.password.chars().count() > 64 || group.password.chars().any(char::is_control) {
            return Err(format!("The password for '{name}' is too long."));
        }
        if passwords.contains(&group.password.as_str()) {
            return Err(format!("'{name}' has the same password as another role."));
        }
        passwords.push(&group.password);
        if !(0..=MAX_SLOTS).contains(&group.reserved_slots) {
            return Err(format!("'{name}' can reserve 0 to {MAX_SLOTS} slots."));
        }
        reserved += group.reserved_slots;
    }
    if reserved > slots {
        return Err(format!(
            "The roles reserve {reserved} slots, and the server has {slots}."
        ));
    }
    Ok(())
}

pub fn set_groups(config: &mut Value, groups: &[Group]) {
    set_path(config, "userGroups", json!(groups));
}

pub fn slots(config: &Value) -> i64 {
    get_path(config, "slotCount")
        .and_then(Value::as_i64)
        .unwrap_or(MAX_SLOTS)
}

/// The ban list, carried as it is, since the shape of an entry has changed between versions.
pub fn bans(config: &Value) -> Vec<Value> {
    config
        .get("bannedAccounts")
        .or_else(|| config.get("bans"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

pub fn remove_ban(config: &mut Value, index: usize) -> Option<Value> {
    let list = ["bannedAccounts", "bans"].into_iter().find_map(|key| {
        config
            .get(key)
            .filter(|value| value.is_array())
            .map(|_| key)
    })?;
    let entries = config.get_mut(list)?.as_array_mut()?;
    (index < entries.len()).then(|| entries.remove(index))
}

/// The log file stamps each line as `[I 00:00:04,524] `; the console stream may not.
pub fn without_stamp(line: &str) -> &str {
    let Some(rest) = line.strip_prefix('[') else {
        return line;
    };
    let Some((stamp, rest)) = rest.split_once("] ") else {
        return line;
    };
    let mut parts = stamp.split(' ');
    let level = parts.next().unwrap_or_default();
    let time = parts.next().unwrap_or_default();
    let stamped = level.len() == 1
        && level.chars().all(|c| c.is_ascii_uppercase())
        && time.contains(':')
        && time
            .chars()
            .all(|c| c.is_ascii_digit() || c == ':' || c == ',');
    match stamped {
        true => rest,
        false => line,
    }
}

/// `[server] Player 'Name' logged in with Permissions:`.
pub fn arrival(line: &str) -> Option<String> {
    let line = without_stamp(line);
    let rest = line.strip_prefix("[server] Player '")?;
    let (name, tail) = rest.rsplit_once('\'')?;
    (tail.trim_start().starts_with("logged in") && !name.is_empty()).then(|| name.to_string())
}

/// `[server] Remove Player 'Name'`, which a player who timed out never gets.
pub fn departure(line: &str) -> Option<String> {
    let line = without_stamp(line);
    let rest = line.strip_prefix("[server] Remove Player '")?;
    let name = rest.strip_suffix('\'')?;
    (!name.is_empty()).then(|| name.to_string())
}

/// The query's version field is a constant; the build is the second keyword, as `1;1265863;…`.
pub fn build_in(keywords: &str) -> Option<String> {
    let build = keywords.split(';').nth(1)?.trim();
    (!build.is_empty() && build.chars().all(|c| c.is_ascii_digit())).then(|| build.to_string())
}

/// `tags` takes the server browser's own words, and an unknown one stops the server starting.
pub fn check_tags(value: &str) -> Result<String, String> {
    let mut chosen: Vec<&str> = Vec::new();
    for tag in value
        .split(',')
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
    {
        let Some(known) = BROWSER_TAGS
            .iter()
            .find(|known| known.eq_ignore_ascii_case(tag))
        else {
            return Err(format!("'{tag}' is not a tag the server browser knows."));
        };
        if !chosen.contains(known) {
            chosen.push(known);
        }
    }
    Ok(chosen.join(","))
}

pub const BROWSER_TAGS: [&str; 19] = [
    "LookingForPlayers",
    "BaseBuilding",
    "Exploration",
    "Roleplay",
    "English",
    "German",
    "French",
    "Italian",
    "Japanese",
    "Korean",
    "Polish",
    "Portuguese",
    "Russian",
    "Spanish",
    "Thai",
    "Turkish",
    "Ukrainian",
    "Chinese",
    "Taiwanese",
];

const CUSTOM: &str = "Counts only under the Custom preset, which changing it here switches to.";

const fn factor(
    key: &'static str,
    label: &'static str,
    group: &'static str,
    min: f64,
    max: f64,
) -> Known {
    Known {
        key,
        label,
        help: CUSTOM,
        group,
        default: "1",
        kind: Kind::Decimal { min, max },
    }
}

const fn flag(
    key: &'static str,
    label: &'static str,
    group: &'static str,
    default: &'static str,
) -> Known {
    Known {
        key,
        label,
        help: CUSTOM,
        group,
        default,
        kind: Kind::Flag,
    }
}

const fn choice(
    key: &'static str,
    label: &'static str,
    group: &'static str,
    default: &'static str,
    options: &'static [&'static str],
) -> Known {
    Known {
        key,
        label,
        help: CUSTOM,
        group,
        default,
        kind: Kind::Choice { options },
    }
}

const fn minutes(
    key: &'static str,
    label: &'static str,
    group: &'static str,
    default: &'static str,
    min: i64,
    max: i64,
) -> Known {
    Known {
        key,
        label,
        help: CUSTOM,
        group,
        default,
        kind: Kind::Number { min, max },
    }
}

/// Every setting in Keen's readme for 1.0, with its documented range.
pub const CATALOGUE: &[Known] = &[
    Known {
        key: "name",
        label: "Name in the server list",
        help: "Leave empty to use the server's name here.",
        group: "Listing",
        default: "",
        kind: Kind::Text,
    },
    Known {
        key: TAGS,
        label: "Tags",
        help: "Comma-separated: LookingForPlayers, BaseBuilding, Exploration, Roleplay, or a language such as English or German.",
        group: "Listing",
        default: "",
        kind: Kind::Text,
    },
    Known {
        key: "slotCount",
        label: "Player slots",
        help: "Slots a role reserves come out of these.",
        group: "Players",
        default: "16",
        kind: Kind::Number {
            min: 1,
            max: MAX_SLOTS,
        },
    },
    Known {
        key: "enableTextChat",
        label: "Text chat",
        help: "",
        group: "Players",
        default: "false",
        kind: Kind::Flag,
    },
    Known {
        key: "enableVoiceChat",
        label: "Voice chat",
        help: "",
        group: "Players",
        default: "false",
        kind: Kind::Flag,
    },
    Known {
        key: "voiceChatMode",
        label: "Voice chat range",
        help: "Proximity is heard nearby only; Global everywhere.",
        group: "Players",
        default: "Proximity",
        kind: Kind::Choice {
            options: &["Proximity", "Global"],
        },
    },
    Known {
        key: PRESET,
        label: "Preset",
        help: "The game's own difficulty presets. Only Custom uses the settings below.",
        group: "World",
        default: "Default",
        kind: Kind::Choice {
            options: &["Default", "Relaxed", "Hard", "Survival", "Custom"],
        },
    },
    minutes(
        "gameSettings.dayTimeDuration",
        "Day length (minutes)",
        "World",
        "30",
        2,
        60,
    ),
    minutes(
        "gameSettings.nightTimeDuration",
        "Night length (minutes)",
        "World",
        "12",
        2,
        60,
    ),
    choice(
        "gameSettings.weatherFrequency",
        "Weather",
        "World",
        "Normal",
        &["Disabled", "Rare", "Normal", "Often"],
    ),
    factor(
        "gameSettings.shroudTimeFactor",
        "Time allowed in the Shroud",
        "World",
        0.5,
        2.0,
    ),
    choice(
        "gameSettings.curseModifier",
        "Shroud curse",
        "World",
        "Normal",
        &["Easy", "Normal", "Hard"],
    ),
    Known {
        key: "gameSettings.flameLevelFogRemoval",
        label: "Flame level that clears the fog",
        help: CUSTOM,
        group: "World",
        default: "11",
        kind: Kind::Number { min: 0, max: 20 },
    },
    factor(
        "gameSettings.playerHealthFactor",
        "Health",
        "Survival",
        0.25,
        4.0,
    ),
    factor(
        "gameSettings.playerManaFactor",
        "Mana",
        "Survival",
        0.25,
        4.0,
    ),
    factor(
        "gameSettings.playerStaminaFactor",
        "Stamina",
        "Survival",
        0.25,
        4.0,
    ),
    factor(
        "gameSettings.playerBodyHeatFactor",
        "Body heat",
        "Survival",
        0.5,
        2.0,
    ),
    factor(
        "gameSettings.playerDivingTimeFactor",
        "Time under water",
        "Survival",
        0.5,
        2.0,
    ),
    flag(
        "gameSettings.enableDurability",
        "Tools and weapons wear out",
        "Survival",
        "true",
    ),
    flag(
        "gameSettings.enableStarvingDebuff",
        "Starving hurts",
        "Survival",
        "false",
    ),
    factor(
        "gameSettings.foodBuffDurationFactor",
        "How long food lasts",
        "Survival",
        0.5,
        2.0,
    ),
    minutes(
        "gameSettings.fromHungerToStarving",
        "From hungry to starving (minutes)",
        "Survival",
        "10",
        5,
        20,
    ),
    choice(
        "gameSettings.tombstoneMode",
        "On death",
        "Survival",
        "AddBackpackMaterials",
        &["AddBackpackMaterials", "Everything", "NoTombstone"],
    ),
    flag(
        "gameSettings.enableGliderTurbulences",
        "Glider turbulence",
        "Survival",
        "true",
    ),
    choice(
        "gameSettings.randomSpawnerAmount",
        "Enemy numbers",
        "Enemies",
        "Automatic",
        &["Few", "Normal", "Automatic", "Many", "Extreme"],
    ),
    choice(
        "gameSettings.aggroPoolAmount",
        "Enemies attacking at once",
        "Enemies",
        "Normal",
        &["Few", "Normal", "Many", "Extreme"],
    ),
    factor(
        "gameSettings.enemyDamageFactor",
        "Enemy damage",
        "Enemies",
        0.25,
        5.0,
    ),
    factor(
        "gameSettings.enemyHealthFactor",
        "Enemy health",
        "Enemies",
        0.25,
        4.0,
    ),
    factor(
        "gameSettings.enemyStaminaFactor",
        "Enemy stamina",
        "Enemies",
        0.5,
        2.0,
    ),
    factor(
        "gameSettings.enemyPerceptionRangeFactor",
        "How far enemies see",
        "Enemies",
        0.5,
        2.0,
    ),
    factor(
        "gameSettings.bossDamageFactor",
        "Boss damage",
        "Enemies",
        0.2,
        5.0,
    ),
    factor(
        "gameSettings.bossHealthFactor",
        "Boss health",
        "Enemies",
        0.2,
        5.0,
    ),
    factor(
        "gameSettings.threatBonus",
        "Threat bonus",
        "Enemies",
        0.25,
        4.0,
    ),
    flag(
        "gameSettings.pacifyAllEnemies",
        "Enemies attack only when attacked",
        "Enemies",
        "false",
    ),
    choice(
        "gameSettings.tamingStartleRepercussion",
        "Startling an animal being tamed",
        "Enemies",
        "LoseSomeProgress",
        &["KeepProgress", "LoseSomeProgress", "LoseAllProgress"],
    ),
    factor(
        "gameSettings.miningDamageFactor",
        "Mining damage",
        "Resources",
        0.5,
        2.0,
    ),
    factor(
        "gameSettings.plantGrowthSpeedFactor",
        "Plant growth",
        "Resources",
        0.25,
        2.0,
    ),
    factor(
        "gameSettings.resourceDropStackAmountFactor",
        "Resources dropped",
        "Resources",
        0.25,
        2.0,
    ),
    factor(
        "gameSettings.factoryProductionSpeedFactor",
        "Crafting station speed",
        "Resources",
        0.25,
        2.0,
    ),
    Known {
        key: "gameSettings.perkUpgradeRecyclingFactor",
        label: "Runes back from salvaged upgrades",
        help: CUSTOM,
        group: "Resources",
        default: "0.5",
        kind: Kind::Decimal { min: 0.0, max: 1.0 },
    },
    factor(
        "gameSettings.perkCostFactor",
        "Upgrade cost",
        "Resources",
        0.25,
        2.0,
    ),
    choice(
        "gameSettings.fishingDifficulty",
        "Fishing",
        "Resources",
        "Normal",
        &["VeryEasy", "Easy", "Normal", "Hard", "VeryHard"],
    ),
    factor(
        "gameSettings.experienceCombatFactor",
        "Experience from combat",
        "Experience",
        0.25,
        2.0,
    ),
    factor(
        "gameSettings.experienceMiningFactor",
        "Experience from mining",
        "Experience",
        0.0,
        2.0,
    ),
    factor(
        "gameSettings.experienceExplorationQuestsFactor",
        "Experience from exploring and quests",
        "Experience",
        0.25,
        2.0,
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_file_has_four_roles_with_their_own_passwords() {
        let config = fresh();
        let roles = groups(&config);
        let names: Vec<&str> = roles.iter().map(|group| group.name.as_str()).collect();
        assert_eq!(names, ["Admin", "Friend", "Guest", "Visitor"]);
        assert!(roles[0].can_kick_ban && !roles[1].can_kick_ban);
        assert!(check_groups(&roles, MAX_SLOTS).is_ok());
        assert!(roles.iter().all(|group| group.password.len() == 12));
    }

    #[test]
    fn values_are_written_as_the_game_types_them() {
        let mut config = fresh();
        apply(
            &mut config,
            &[
                ("slotCount".into(), "8".into()),
                ("enableTextChat".into(), "true".into()),
                ("gameSettings.playerHealthFactor".into(), "1.5".into()),
                ("gameSettings.dayTimeDuration".into(), "45".into()),
                ("tags".into(), "English,BaseBuilding".into()),
            ],
        );
        assert_eq!(config["slotCount"], json!(8));
        assert_eq!(config["enableTextChat"], json!(true));
        assert_eq!(config["gameSettings"]["playerHealthFactor"], json!(1.5));
        assert_eq!(
            config["gameSettings"]["dayTimeDuration"],
            json!(2_700_000_000_000_i64)
        );
        assert_eq!(config["tags"], json!(["English", "BaseBuilding"]));
        assert_eq!(
            shown(&config, "gameSettings.dayTimeDuration").as_deref(),
            Some("45")
        );
        assert_eq!(
            shown(&config, "tags").as_deref(),
            Some("English,BaseBuilding")
        );
    }

    #[test]
    fn a_game_setting_switches_to_the_custom_preset() {
        let mut config = fresh();
        apply(
            &mut config,
            &[("gameSettings.enemyDamageFactor".into(), "2".into())],
        );
        assert_eq!(config[PRESET], json!("Custom"));

        let mut config = fresh();
        apply(
            &mut config,
            &[
                ("gameSettings.enemyDamageFactor".into(), "2".into()),
                (PRESET.into(), "Hard".into()),
            ],
        );
        assert_eq!(config[PRESET], json!("Hard"));
    }

    #[test]
    fn an_empty_value_hands_the_key_back_to_the_game() {
        let mut config = fresh();
        set(&mut config, "gameSettings.bossHealthFactor", "3");
        set(&mut config, "gameSettings.bossHealthFactor", "");
        assert!(config["gameSettings"].get("bossHealthFactor").is_none());
        set(&mut config, "slotCount", "");
        assert!(config.get("slotCount").is_none());
    }

    #[test]
    fn a_key_the_panel_does_not_know_keeps_its_type() {
        let mut config = json!({ "futureNumber": 3, "futureFlag": false, "futureText": "a" });
        set(&mut config, "futureNumber", "5");
        set(&mut config, "futureFlag", "true");
        set(&mut config, "futureText", "b");
        assert_eq!(
            config,
            json!({ "futureNumber": 5, "futureFlag": true, "futureText": "b" })
        );
        let names: Vec<String> = others(&config).into_iter().map(|(key, _)| key).collect();
        assert_eq!(names, ["futureFlag", "futureNumber", "futureText"]);
    }

    #[test]
    fn big_ids_in_the_ban_list_survive_a_round_trip() {
        let text = r#"{"bannedAccounts":[{"accountId":76561198000000001,"displayName":"x"}]}"#;
        let mut config: Value = serde_json::from_str(text).unwrap();
        assert_eq!(bans(&config).len(), 1);
        assert_eq!(serde_json::to_string(&config).unwrap(), text);
        assert!(remove_ban(&mut config, 1).is_none());
        assert!(remove_ban(&mut config, 0).is_some());
        assert!(bans(&config).is_empty());
    }

    #[test]
    fn roles_need_their_own_names_and_passwords() {
        let mut roles = groups(&fresh());
        roles[1].password = roles[0].password.clone();
        assert!(check_groups(&roles, MAX_SLOTS).is_err());

        let mut roles = groups(&fresh());
        roles[2].name = "admin".into();
        assert!(check_groups(&roles, MAX_SLOTS).is_err());

        let mut roles = groups(&fresh());
        roles[0].reserved_slots = 6;
        roles[1].reserved_slots = 6;
        assert!(check_groups(&roles, 16).is_ok());
        assert!(check_groups(&roles, 10).is_err());
    }

    #[test]
    fn a_role_keeps_fields_a_later_version_adds() {
        let value = json!([{ "name": "Admin", "password": "a", "canFly": true }]);
        let mut config = json!({ "userGroups": value });
        let roles = groups(&config);
        set_groups(&mut config, &roles);
        assert_eq!(config["userGroups"][0]["canFly"], json!(true));
    }

    #[test]
    fn reads_arrivals_and_departures_with_or_without_the_stamp() {
        assert_eq!(
            arrival("[server] Player 'Wanderer' logged in with Permissions:").as_deref(),
            Some("Wanderer")
        );
        assert_eq!(
            arrival("[I 00:12:04,524] [server] Player 'Two Words' logged in with Permissions:")
                .as_deref(),
            Some("Two Words")
        );
        assert_eq!(
            arrival("[server] Machine '1': Player '0(0)' logged in"),
            None
        );
        assert_eq!(
            departure("[server] Remove Player 'Wanderer'").as_deref(),
            Some("Wanderer")
        );
        assert_eq!(
            departure("[server] Remove Entity for Player 'Wanderer'"),
            None
        );
        assert_eq!(
            without_stamp("[Session] 'HostOnline' (up)!"),
            "[Session] 'HostOnline' (up)!"
        );
    }

    #[test]
    fn the_build_comes_from_the_keywords() {
        assert_eq!(
            build_in("1;1265863;1;0;1;0;0;0;0").as_deref(),
            Some("1265863")
        );
        assert_eq!(build_in(""), None);
    }

    #[test]
    fn tags_are_the_browser_s_own_words() {
        assert_eq!(
            check_tags(" english, BaseBuilding,English").unwrap(),
            "English,BaseBuilding"
        );
        assert!(check_tags("Hardcore").is_err());
        assert_eq!(check_tags("").unwrap(), "");
    }

    #[test]
    fn under_wine_the_server_starts_from_its_own_folder() {
        let command = start_command(Path::new("servers/one"), Path::new("/config/wine"));
        let words = shell_words::split(&command).unwrap();
        if cfg!(windows) {
            assert!(words[0].ends_with(executable()));
        } else {
            assert_eq!(words[0], "env");
            assert!(words.contains(&"WINEPREFIX=/config/wine".to_string()));
            assert_eq!(&words[words.len() - 2..], ["wine", executable()]);
        }
    }

    #[test]
    fn every_catalogue_default_passes_its_own_check() {
        for entry in CATALOGUE {
            if entry.default.is_empty() {
                continue;
            }
            assert!(
                crate::properties::check(entry, entry.default).is_ok(),
                "{}",
                entry.key
            );
        }
    }
}
