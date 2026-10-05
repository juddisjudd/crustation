//! What the panel knows about the game Rust: where its ports sit and how it is started.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::Path;

use crate::rcon::Endpoint;

pub const KIND: &str = "rust";
pub const APP_ID: u32 = 258550;
pub const DEFAULT_PORT: i64 = 28015;

/// Rust takes three ports and the panel keeps them in the order the game's own
/// defaults do (28015, 28016, 28017), so one number on the form settles all three.
pub fn rcon_port(port: i64) -> i64 {
    port + 1
}

pub fn query_port(port: i64) -> i64 {
    port + 2
}

pub fn executable() -> &'static str {
    if cfg!(windows) {
        "RustDedicated.exe"
    } else {
        "RustDedicated"
    }
}

/// The whole command line. Rust has no settings file the panel owns, so everything
/// it is told at creation lives here, and RCON is read back out of it.
pub fn start_command(
    directory: &Path,
    steam_home: &Path,
    name: &str,
    port: i64,
    settings: &[(String, String)],
    rcon_password: &str,
) -> String {
    let mut convars: Vec<(String, String)> = vec![
        ("server.port".into(), port.to_string()),
        ("server.queryport".into(), query_port(port).to_string()),
        ("rcon.ip".into(), "127.0.0.1".into()),
        ("rcon.port".into(), rcon_port(port).to_string()),
        ("rcon.password".into(), rcon_password.into()),
        // Websocket RCON keeps the panel's commands out of the console; the other is deprecated.
        ("rcon.web".into(), "1".into()),
        ("server.identity".into(), "crustation".into()),
        ("server.hostname".into(), name.into()),
    ];
    for (key, value) in settings {
        if let Some(at) = convars.iter().position(|(existing, _)| existing == key) {
            convars.remove(at);
        }
        if !omitted(key, value) {
            convars.push((key.clone(), value.clone()));
        }
    }

    let mut parts: Vec<String> = Vec::new();
    if cfg!(windows) {
        parts.push(directory.join(executable()).to_string_lossy().into_owned());
    } else {
        // The supervisor starts a bare program, so `env` is how the library path and
        // the Steam home reach the server without teaching it about environments.
        parts.push("env".into());
        parts.push(format!("HOME={}", steam_home.display()));
        parts.push("LD_LIBRARY_PATH=.:./RustDedicated_Data/Plugins/x86_64".into());
        parts.push(format!("./{}", executable()));
    }
    parts.extend(["-batchmode".to_string(), "-nographics".to_string()]);
    for (key, value) in convars {
        parts.push(format!("+{key}"));
        parts.push(value);
    }
    shell_words::join(parts)
}

/// A setting with nothing to say is left off the command, so the server's own default
/// applies. Rust does not know the word `standard`, which stands for no game mode at all.
fn omitted(key: &str, value: &str) -> bool {
    value.trim().is_empty() || (key == "server.gamemode" && value == "standard")
}

/// What the settings screen shows but does not let anyone change. The panel keeps
/// these in step with the server's row and with each other.
pub const MANAGED: &[&str] = &[
    "server.port",
    "server.queryport",
    "server.identity",
    "rcon.ip",
    "rcon.port",
    "rcon.password",
    "rcon.web",
    crate::rust_moderation::REPORTS_URL,
    crate::rust_moderation::REPORTS_KEY,
    crate::rust_moderation::BANS_URL,
    crate::rust_moderation::BANS_FAILURE,
];

/// Settings nobody but the server and the panel should read.
const SECRETS: [&str; 2] = ["rcon.password", crate::rust_moderation::REPORTS_KEY];

pub fn is_managed(key: &str) -> bool {
    MANAGED.contains(&key)
}

pub fn is_secret(key: &str) -> bool {
    SECRETS.contains(&key)
}

enum Part {
    /// The program, its environment and its `-flags`.
    Bare(String),
    /// A `+name value` pair. The value is taken whatever it looks like.
    Convar(String, String),
}

fn parse(command: &str) -> Option<Vec<Part>> {
    let mut words = shell_words::split(command).ok()?.into_iter();
    let mut parts = Vec::new();
    while let Some(word) = words.next() {
        match word.strip_prefix('+').filter(|key| !key.is_empty()) {
            Some(key) => {
                let value = words.next().unwrap_or_default();
                parts.push(Part::Convar(key.to_string(), value));
            }
            None => parts.push(Part::Bare(word)),
        }
    }
    Some(parts)
}

fn render(parts: Vec<Part>) -> String {
    let words = parts.into_iter().flat_map(|part| match part {
        Part::Bare(word) => vec![word],
        Part::Convar(key, value) => vec![format!("+{key}"), value],
    });
    shell_words::join(words)
}

/// The settings on a start command, without the RCON password, which nobody reading
/// the screen needs and everybody who could read the screen should not be handed.
pub fn visible_convars(command: &str) -> Vec<(String, String)> {
    parse(command)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|part| match part {
            Part::Convar(key, value) if !SECRETS.contains(&key.as_str()) => Some((key, value)),
            _ => None,
        })
        .collect()
}

/// Sets or removes settings on a start command. `None`, an empty value, and a value
/// the server would treat as its default all take the setting off. Returns `None`
/// when the command cannot be read, which is better than writing half of one back.
pub fn with_changes(command: &str, changes: &[(String, Option<String>)]) -> Option<String> {
    let mut parts = parse(command)?;
    for (key, value) in changes {
        let is_key = |part: &Part| matches!(part, Part::Convar(existing, _) if existing == key);
        let at = parts.iter().position(is_key);
        parts.retain(|part| !is_key(part));

        let Some(value) = value.as_ref().filter(|value| !omitted(key, value)) else {
            continue;
        };
        let part = Part::Convar(key.clone(), value.clone());
        match at {
            Some(at) => parts.insert(at, part),
            None => parts.push(part),
        }
    }
    Some(render(parts))
}

/// Sets or removes `env` variables on the start command; `None` when it has no `env` to hold them.
pub fn with_env(command: &str, changes: &[(&str, Option<String>)]) -> Option<String> {
    let mut parts = parse(command)?;
    if !matches!(parts.first(), Some(Part::Bare(first)) if first == "env") {
        return None;
    }
    let assignment = |part: &Part| matches!(part, Part::Bare(word) if word.contains('='));
    for (key, value) in changes {
        let prefix = format!("{key}=");
        parts.retain(|part| !matches!(part, Part::Bare(word) if word.starts_with(&prefix)));
        if let Some(value) = value {
            let program = parts
                .iter()
                .skip(1)
                .position(|part| !assignment(part))
                .map_or(parts.len(), |at| at + 1);
            parts.insert(program, Part::Bare(format!("{key}={value}")));
        }
    }
    Some(render(parts))
}

/// Moves the three ports to follow the game port.
pub fn with_port(command: &str, port: i64) -> Option<String> {
    with_changes(
        command,
        &[
            ("server.port".into(), Some(port.to_string())),
            (
                "server.queryport".into(),
                Some(query_port(port).to_string()),
            ),
            ("rcon.port".into(), Some(rcon_port(port).to_string())),
        ],
    )
}

/// A setting the catalogue does not know. The command line is the only rule left,
/// so hold it to that: a name the game could have, and a value on one line.
pub fn check_other(key: &str, value: &str) -> Result<String, String> {
    let named_like_a_convar = !key.is_empty()
        && key.len() <= 128
        && key
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '.' || ch == '_');
    if !named_like_a_convar {
        return Err("A name can hold letters, digits, dots and underscores only.".into());
    }
    if value.chars().any(|ch| ch.is_control()) {
        return Err("A value has to stay on one line.".into());
    }
    if value.chars().count() > 1024 {
        return Err("That is too long.".into());
    }
    Ok(value.trim().to_string())
}

/// The RCON listener a start command asks for. Reading it from the command means
/// an operator who edits the port or password there is followed without a second copy.
pub fn rcon_endpoint(command: &str) -> Option<Endpoint> {
    let words = shell_words::split(command).ok()?;
    let value = |key: &str| {
        let at = words.iter().position(|word| word == key)?;
        words.get(at + 1).cloned()
    };

    let port: u16 = value("+rcon.port")?.parse().ok()?;
    let password = value("+rcon.password").filter(|password| !password.is_empty())?;
    Some(Endpoint {
        address: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port),
        password,
    })
}

/// The server browser's tag groups; an exclusive group takes one tag at most.
pub const TAG_GROUPS: [(&str, bool, &[&str]); 5] = [
    ("wipe", true, &["monthly", "biweekly", "weekly"]),
    ("difficulty", true, &["vanilla", "hardcore", "softcore"]),
    (
        "gameplay",
        false,
        &[
            "pve",
            "roleplay",
            "creative",
            "minigame",
            "training",
            "battlefield",
            "broyale",
            "builds",
            "tut",
        ],
    ),
    ("modding", false, &["premium", "modded", "oxide", "carbon"]),
    ("region", true, &["na", "sa", "eu", "wa", "ea", "oc", "af"]),
];

/// `server.tags` as the browser reads it: lower case, comma-separated, one per exclusive group.
pub fn check_tags(value: &str) -> Result<String, String> {
    let mut tags: Vec<String> = Vec::new();
    for tag in value.split(',').map(|tag| tag.trim().to_lowercase()) {
        if tag.is_empty() || tags.contains(&tag) {
            continue;
        }
        if !tag.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err(format!("'{tag}' is not a tag the browser knows."));
        }
        tags.push(tag);
    }
    for (group, exclusive, members) in TAG_GROUPS {
        let chosen: Vec<&String> = tags
            .iter()
            .filter(|tag| members.contains(&tag.as_str()))
            .collect();
        if exclusive && chosen.len() > 1 {
            return Err(format!("Choose one {group} tag, not {}.", chosen.len()));
        }
    }
    Ok(tags.join(","))
}

/// One setting's value on the start command, secrets included.
pub fn convar(command: &str, key: &str) -> Option<String> {
    parse(command)?.into_iter().find_map(|part| match part {
        Part::Convar(name, value) if name == key && !value.is_empty() => Some(value),
        _ => None,
    })
}

/// Whether the command asks for websocket RCON rather than the legacy Source flavour.
pub fn rcon_web(command: &str) -> bool {
    let Ok(words) = shell_words::split(command) else {
        return false;
    };
    words
        .iter()
        .position(|word| word == "+rcon.web")
        .and_then(|at| words.get(at + 1))
        .is_some_and(|value| value == "1" || value.eq_ignore_ascii_case("true"))
}

/// The command with the RCON password hidden, for anyone who is not an administrator.
/// Seeing it would let them run console commands without holding that permission.
pub fn masked(command: &str) -> String {
    let Ok(mut words) = shell_words::split(command) else {
        return String::new();
    };
    for at in 0..words.len().saturating_sub(1) {
        if SECRETS.iter().any(|key| words[at] == format!("+{key}")) {
            words[at + 1] = "********".into();
        }
    }
    shell_words::join(words)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command(settings: &[(String, String)]) -> String {
        start_command(
            Path::new("servers/one"),
            Path::new("/config/steamcmd/home"),
            "My Rust Server",
            28015,
            settings,
            "hunter2",
        )
    }

    fn argument_after(command: &str, key: &str) -> Option<String> {
        let words = shell_words::split(command).unwrap();
        let at = words.iter().position(|word| word == key)?;
        words.get(at + 1).cloned()
    }

    #[test]
    fn the_three_ports_follow_the_one_asked_for() {
        let command = command(&[]);
        assert_eq!(argument_after(&command, "+server.port").unwrap(), "28015");
        assert_eq!(argument_after(&command, "+rcon.port").unwrap(), "28016");
        assert_eq!(
            argument_after(&command, "+server.queryport").unwrap(),
            "28017"
        );
    }

    #[test]
    fn a_name_with_spaces_stays_one_argument() {
        let command = command(&[]);
        assert_eq!(
            argument_after(&command, "+server.hostname").unwrap(),
            "My Rust Server"
        );
    }

    #[test]
    fn a_setting_cannot_smuggle_in_another() {
        let hostile = [(
            "server.description".to_string(),
            "x +rcon.password pwned".to_string(),
        )];
        let command = command(&hostile);
        assert_eq!(
            argument_after(&command, "+server.description").unwrap(),
            "x +rcon.password pwned"
        );
        assert_eq!(rcon_endpoint(&command).unwrap().password, "hunter2");
    }

    #[test]
    fn rcon_is_read_back_out_of_the_command() {
        let endpoint = rcon_endpoint(&command(&[])).unwrap();
        assert_eq!(endpoint.address.port(), 28016);
        assert!(endpoint.address.ip().is_loopback());
        assert_eq!(endpoint.password, "hunter2");
    }

    #[test]
    fn the_password_is_hidden_and_the_rest_is_left_alone() {
        let shown = masked(&command(&[]));
        assert!(!shown.contains("hunter2"));
        assert_eq!(
            argument_after(&shown, "+rcon.password").unwrap(),
            "********"
        );
        assert_eq!(argument_after(&shown, "+rcon.port").unwrap(), "28016");
    }

    #[test]
    fn a_setting_is_replaced_in_place_and_the_rest_left_alone() {
        let before = command(&[("server.maxplayers".into(), "10".into())]);
        let after =
            with_changes(&before, &[("server.maxplayers".into(), Some("100".into()))]).unwrap();
        assert_eq!(argument_after(&after, "+server.maxplayers").unwrap(), "100");
        assert_eq!(argument_after(&after, "+rcon.password").unwrap(), "hunter2");
        assert_eq!(after.matches("server.maxplayers").count(), 1);
    }

    #[test]
    fn an_empty_value_takes_the_setting_off() {
        let before = command(&[("server.seed".into(), "1234".into())]);
        let after = with_changes(&before, &[("server.seed".into(), Some(String::new()))]).unwrap();
        assert!(argument_after(&after, "+server.seed").is_none());
    }

    #[test]
    fn the_standard_game_mode_is_no_game_mode() {
        let with = command(&[("server.gamemode".into(), "softcore".into())]);
        assert_eq!(
            argument_after(&with, "+server.gamemode").unwrap(),
            "softcore"
        );
        let without = with_changes(
            &with,
            &[("server.gamemode".into(), Some("standard".into()))],
        )
        .unwrap();
        assert!(argument_after(&without, "+server.gamemode").is_none());
    }

    #[test]
    fn a_value_that_looks_like_a_setting_stays_a_value() {
        let after = with_changes(
            &command(&[]),
            &[("server.description".into(), Some("+rcon.port 1".into()))],
        )
        .unwrap();
        assert_eq!(argument_after(&after, "+rcon.port").unwrap(), "28016");
        assert_eq!(
            argument_after(&after, "+server.description").unwrap(),
            "+rcon.port 1"
        );
    }

    #[test]
    fn moving_the_port_moves_all_three() {
        let after = with_port(&command(&[]), 30000).unwrap();
        assert_eq!(argument_after(&after, "+server.port").unwrap(), "30000");
        assert_eq!(argument_after(&after, "+rcon.port").unwrap(), "30001");
        assert_eq!(
            argument_after(&after, "+server.queryport").unwrap(),
            "30002"
        );
        assert_eq!(rcon_endpoint(&after).unwrap().address.port(), 30001);
    }

    #[test]
    fn the_password_is_not_among_the_settings_shown() {
        let shown = visible_convars(&command(&[]));
        assert!(shown.iter().all(|(key, _)| key != "rcon.password"));
        assert!(shown.iter().any(|(key, _)| key == "server.port"));
    }

    #[test]
    fn a_setting_is_named_like_a_convar_or_refused() {
        assert!(check_other("decay.scale", "2").is_ok());
        assert!(check_other("two words", "x").is_err());
        assert!(check_other("-logfile", "x").is_err());
        assert!(check_other("ok.name", "line\nbreak").is_err());
    }

    #[test]
    fn a_command_without_rcon_has_no_endpoint() {
        assert!(rcon_endpoint("./RustDedicated -batchmode").is_none());
    }

    #[test]
    fn sets_and_clears_environment_on_the_start_command() {
        let before = "env HOME=/home LD_LIBRARY_PATH=. ./RustDedicated -batchmode +rcon.password hunter2 +rcon.port 28016";
        let after = with_env(
            before,
            &[
                ("DOORSTOP_ENABLED", Some("1".into())),
                ("TERM", Some("xterm".into())),
            ],
        )
        .unwrap();
        let words = shell_words::split(&after).unwrap();
        let program = words
            .iter()
            .position(|word| word == "./RustDedicated")
            .unwrap();
        assert!(words[..program].contains(&"DOORSTOP_ENABLED=1".to_string()));
        assert!(words[..program].contains(&"TERM=xterm".to_string()));
        assert_eq!(rcon_endpoint(&after).unwrap().password, "hunter2");

        let cleared = with_env(&after, &[("DOORSTOP_ENABLED", None), ("TERM", None)]).unwrap();
        assert_eq!(
            shell_words::split(&cleared).unwrap(),
            shell_words::split(before).unwrap()
        );
        assert!(with_env("./RustDedicated -batchmode", &[("TERM", None)]).is_none());
    }

    #[test]
    fn the_report_key_stays_out_of_sight() {
        let command = "./RustDedicated +server.reportsserverendpointkey s3cret +server.hostname x";
        assert!(!masked(command).contains("s3cret"));
        assert!(
            visible_convars(command)
                .iter()
                .all(|(key, _)| key != crate::rust_moderation::REPORTS_KEY)
        );
        assert_eq!(
            convar(command, crate::rust_moderation::REPORTS_KEY).as_deref(),
            Some("s3cret")
        );
    }

    #[test]
    fn tags_take_one_from_each_exclusive_group() {
        assert_eq!(
            check_tags(" Weekly, vanilla,EU, pve,pve ").unwrap(),
            "weekly,vanilla,eu,pve"
        );
        assert_eq!(
            check_tags("pve,roleplay,oxide,modded").unwrap(),
            "pve,roleplay,oxide,modded"
        );
        assert!(check_tags("weekly,monthly").is_err());
        assert!(check_tags("na,eu").is_err());
        assert!(check_tags("bad tag").is_err());
        assert_eq!(check_tags("").unwrap(), "");
    }

    #[test]
    fn new_servers_speak_websocket_rcon() {
        assert!(rcon_web(&command(&[])));
        assert!(!rcon_web("./RustDedicated +rcon.web 0"));
        assert!(!rcon_web("./RustDedicated"));
    }
}
