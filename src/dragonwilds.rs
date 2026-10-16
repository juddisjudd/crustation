//! Jagex's RuneScape: Dragonwilds: an Unreal server with its settings in one INI file.

use std::borrow::Cow;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::properties::{Kind, Known};

pub const KIND: &str = "dragonwilds";
pub const APP_ID: u32 = 4019830;
pub const DEFAULT_PORT: i64 = 7777;
/// Jagex's own container keeps the world settings beacon this far above the game port.
pub const BEACON_OFFSET: i64 = 1111;
/// The build caps a world at six, whatever the settings say.
pub const MAX_PLAYERS: i64 = 6;
pub const SECTION: &str = "/Script/Dominion.DedicatedServerSettings";

/// The game binary itself, since the launcher script does not pass signals on to it.
pub fn executable() -> &'static str {
    if cfg!(windows) {
        "RSDragonwilds/Binaries/Win64/RSDragonwildsServer-Win64-Shipping.exe"
    } else {
        "RSDragonwilds/Binaries/Linux/RSDragonwildsServer-Linux-Shipping"
    }
}

/// Files that have to be executable besides the server, or a crash takes the process with it.
pub fn helpers() -> [&'static str; 1] {
    ["RSDragonwilds/Plugins/Developer/Sentry/Binaries/Linux/crashpad_handler"]
}

pub fn beacon_port(port: i64) -> i64 {
    port + BEACON_OFFSET
}

pub fn config_path(directory: &Path) -> PathBuf {
    let platform = if cfg!(windows) {
        "WindowsServer"
    } else {
        "LinuxServer"
    };
    directory
        .join("RSDragonwilds")
        .join("Saved")
        .join("Config")
        .join(platform)
        .join("DedicatedServer.ini")
}

/// Linux logs to standard output under `-log`; Windows opens a window for that unless told otherwise.
pub fn start_command(directory: &Path, steam_home: &Path, port: i64) -> String {
    let mut parts: Vec<String> = Vec::new();
    if cfg!(windows) {
        parts.push(directory.join(executable()).to_string_lossy().into_owned());
    } else {
        // The Steam client library is found under $HOME/.steam/sdk64, where SteamCMD's copy is linked.
        parts.push("env".into());
        parts.push(format!("HOME={}", steam_home.display()));
        parts.push(format!("./{}", executable()));
    }
    parts.push("RSDragonwilds".into());
    if cfg!(windows) {
        parts.extend(["-stdout".into(), "-FullStdOutLogOutput".into()]);
    } else {
        parts.push("-log".into());
    }
    parts.push(format!("-Port={port}"));
    parts.push(format!("-BeaconPort={}", beacon_port(port)));
    shell_words::join(parts)
}

/// Moves both ports to follow the game port; `None` when the command cannot be read.
pub fn with_port(command: &str, port: i64) -> Option<String> {
    let mut words = shell_words::split(command).ok()?;
    let mut seen = false;
    for word in &mut words {
        let flag = word
            .split('=')
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        match flag.as_str() {
            "-port" => {
                *word = format!("-Port={port}");
                seen = true;
            }
            "-beaconport" => *word = format!("-BeaconPort={}", beacon_port(port)),
            _ => {}
        }
    }
    seen.then(|| shell_words::join(words))
}

/// The beacon port a start command names, which an operator may have moved by hand.
pub fn beacon_in(command: &str) -> Option<i64> {
    shell_words::split(command).ok()?.iter().find_map(|word| {
        let (flag, value) = word.split_once('=')?;
        flag.eq_ignore_ascii_case("-BeaconPort")
            .then(|| value.parse().ok())
            .flatten()
    })
}

/// An Unreal INI file held line by line, so writing it back disturbs only the keys that changed.
#[derive(Debug, Clone, Default)]
pub struct Ini {
    lines: Vec<String>,
    crlf: bool,
}

impl Ini {
    pub fn parse(text: &str) -> Self {
        Self {
            lines: text
                .trim_start_matches('\u{feff}')
                .lines()
                .map(str::to_string)
                .collect(),
            crlf: text.contains("\r\n"),
        }
    }

    pub fn render(&self) -> String {
        let separator = if self.crlf { "\r\n" } else { "\n" };
        let mut out = self.lines.join(separator);
        out.push_str(separator);
        out
    }

    /// The header's line and the line after the section's last entry.
    fn section(&self, name: &str) -> Option<(usize, usize)> {
        let header = format!("[{name}]");
        let start = self.lines.iter().position(|line| line.trim() == header)?;
        let end = self.lines[start + 1..]
            .iter()
            .position(|line| line.trim_start().starts_with('['))
            .map_or(self.lines.len(), |at| start + 1 + at);
        let last = self.lines[start + 1..end]
            .iter()
            .rposition(|line| !line.trim().is_empty())
            .map_or(start + 1, |at| start + 2 + at);
        Some((start, last))
    }

    fn find(&self, section: &str, key: &str) -> Option<usize> {
        let (start, end) = self.section(section)?;
        (start + 1..end).find(|&at| entry(&self.lines[at]).is_some_and(|(name, _)| name == key))
    }

    pub fn get(&self, section: &str, key: &str) -> Option<String> {
        let at = self.find(section, key)?;
        entry(&self.lines[at]).map(|(_, value)| value.to_string())
    }

    pub fn set(&mut self, section: &str, key: &str, value: &str) {
        let line = format!("{key}={value}");
        if let Some(at) = self.find(section, key) {
            self.lines[at] = line;
            return;
        }
        match self.section(section) {
            Some((_, end)) => self.lines.insert(end, line),
            None => {
                if self
                    .lines
                    .last()
                    .is_some_and(|last| !last.trim().is_empty())
                {
                    self.lines.push(String::new());
                }
                self.lines.push(format!("[{section}]"));
                self.lines.push(line);
            }
        }
    }

    pub fn remove(&mut self, section: &str, key: &str) {
        if let Some(at) = self.find(section, key) {
            self.lines.remove(at);
        }
    }

    /// Every `key=value` in a section, in order, repeated keys included.
    pub fn entries(&self, section: &str) -> Vec<(String, String)> {
        let Some((start, end)) = self.section(section) else {
            return Vec::new();
        };
        self.lines[start + 1..end]
            .iter()
            .filter_map(|line| entry(line))
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect()
    }
}

fn entry(line: &str) -> Option<(&str, &str)> {
    let trimmed = line.trim_start();
    if trimmed.is_empty() || trimmed.starts_with(';') || trimmed.starts_with('[') {
        return None;
    }
    let (key, value) = trimmed.split_once('=')?;
    Some((key.trim(), value.trim()))
}

pub async fn load(directory: &Path) -> Result<Option<Ini>> {
    let path = config_path(directory);
    match tokio::fs::read_to_string(&path).await {
        Ok(text) => Ok(Some(Ini::parse(&text))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| format!("reading {}", path.display())),
    }
}

pub async fn save(directory: &Path, ini: &Ini) -> Result<()> {
    let path = config_path(directory);
    if let Some(folder) = path.parent() {
        tokio::fs::create_dir_all(folder)
            .await
            .with_context(|| format!("creating {}", folder.display()))?;
    }
    let temporary = path.with_extension("ini.tmp");
    tokio::fs::write(&temporary, ini.render())
        .await
        .with_context(|| format!("writing {}", temporary.display()))?;
    tokio::fs::rename(&temporary, &path)
        .await
        .with_context(|| format!("replacing {}", path.display()))
}

/// Writes what the server needs before its first start; it generates the GUID itself.
pub async fn configure(directory: &Path, name: &str, settings: &[(String, String)]) -> Result<()> {
    let mut ini = load_with_admin(directory).await?.unwrap_or_default();
    ini.set(SECTION, "ServerName", name);
    if ini
        .get(SECTION, "DefaultWorldName")
        .is_none_or(|world| world.is_empty())
    {
        ini.set(SECTION, "DefaultWorldName", &world_name_from(name));
    }
    if ini
        .get(SECTION, "AdminPassword")
        .is_none_or(|password| password.is_empty())
    {
        ini.set(SECTION, "AdminPassword", &admin_password());
    }
    for (key, value) in settings {
        set(&mut ini, key, value);
    }
    save_with_admin(directory, &ini).await
}

/// An empty value takes the key out, so the server generates or defaults it again.
pub fn set(ini: &mut Ini, key: &str, value: &str) {
    match value.trim().is_empty() {
        true => ini.remove(SECTION, key),
        false => ini.set(SECTION, key, value.trim()),
    }
}

fn admin_password() -> String {
    crate::rcon::generate_password().chars().take(16).collect()
}

/// The game reads its admin password at start and leaves it out when it rewrites the file.
fn kept_admin_password(directory: &Path) -> PathBuf {
    config_path(directory).with_file_name("crustation-admin-password")
}

/// The file as the panel shows and edits it, with the admin password the game dropped put back.
pub async fn load_with_admin(directory: &Path) -> Result<Option<Ini>> {
    let Some(mut ini) = load(directory).await? else {
        return Ok(None);
    };
    if ini.get(SECTION, "AdminPassword").is_none()
        && let Ok(kept) = tokio::fs::read_to_string(kept_admin_password(directory)).await
        && !kept.trim().is_empty()
    {
        ini.set(SECTION, "AdminPassword", kept.trim());
    }
    Ok(Some(ini))
}

/// Saves the file and keeps a copy of its admin password, or forgets the copy when it has none.
pub async fn save_with_admin(directory: &Path, ini: &Ini) -> Result<()> {
    save(directory, ini).await?;
    let kept = kept_admin_password(directory);
    match ini
        .get(SECTION, "AdminPassword")
        .filter(|password| !password.is_empty())
    {
        Some(password) => tokio::fs::write(&kept, password)
            .await
            .with_context(|| format!("writing {}", kept.display())),
        None => {
            tokio::fs::remove_file(&kept).await.ok();
            Ok(())
        }
    }
}

/// Puts the admin password back before a start, as Jagex's own container does on every launch.
pub async fn before_start(directory: &Path) -> Result<()> {
    let written = load(directory).await?;
    if let (Some(written), Some(shown)) = (written, load_with_admin(directory).await?)
        && written.get(SECTION, "AdminPassword") != shown.get(SECTION, "AdminPassword")
    {
        save(directory, &shown).await?;
    }
    Ok(())
}

/// The world name is what players search for and what the save file is called.
pub fn world_name_from(name: &str) -> String {
    let kept: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == ' ' || *c == '-' || *c == '_')
        .take(16)
        .collect();
    match kept.trim() {
        "" => "World".into(),
        trimmed => trimmed.to_string(),
    }
}

/// The Player ID at the bottom of the game's settings: an Epic account id, 32 hex digits.
pub fn check_owner(value: &str) -> Result<String, String> {
    let value = value.trim().to_ascii_lowercase();
    if value.len() != 32 || !value.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("A Player ID is 32 letters and digits, copied from the bottom of the game's Settings menu.".into());
    }
    Ok(value)
}

/// The save file takes this name, so it holds to what a file name can.
pub fn check_world(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > 64 {
        return Err("Give the world a name of 64 characters or fewer.".into());
    }
    if !value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == ' ' || c == '-' || c == '_')
    {
        return Err(
            "A world name can hold letters, digits, spaces, dashes and underscores.".into(),
        );
    }
    Ok(value.to_string())
}

/// Written by the server: who has been on, with their bans, and the id clients know the server by.
pub const MANAGED: [&str; 2] = ["KnownPlayerList", "ServerGuid"];

pub fn is_managed(key: &str) -> bool {
    MANAGED.contains(&key.trim_start_matches(['+', '-', '.', '!']))
}

/// `LogDomMatcherSession: Player ADDED to session [<id>]-[Name]`, as (id, name).
pub fn arrival(line: &str) -> Option<(String, String)> {
    player_in(line, "Player ADDED to session [")
}

/// The same line with `Removed from`, which a timeout or a crash also produces.
pub fn departure(line: &str) -> Option<(String, String)> {
    player_in(line, "Player Removed from session [")
}

fn player_in(line: &str, marker: &str) -> Option<(String, String)> {
    let (_, rest) = line.split_once(marker)?;
    let (id, rest) = rest.split_once("]-[")?;
    let name = rest.strip_suffix(']').unwrap_or(rest).trim();
    let id = id.trim();
    (!id.is_empty() && !name.is_empty()).then(|| (id.to_string(), name.to_string()))
}

/// Unreal marks its chattiest lines, the session heartbeats among them, as Verbose.
pub fn is_noise(line: &str) -> bool {
    line.contains(": Verbose: ") || line.contains(": VeryVerbose: ")
}

const HIDDEN: &str = "********";

/// The server prints the world password at start and in every login request; the console hides it.
pub fn redact(line: &str) -> Cow<'_, str> {
    let lower = line.to_ascii_lowercase();
    if !lower.contains("password") && !lower.contains("?p=") {
        return Cow::Borrowed(line);
    }
    let mut out = String::with_capacity(line.len());
    let mut at = 0;
    while at < line.len() {
        let rest = &lower[at..];
        if let Some(after) = rest
            .strip_prefix("password: ")
            .map(|_| at + "password: ".len())
        {
            out.push_str(&line[at..after]);
            out.push_str(HIDDEN);
            return Cow::Owned(out);
        }
        if rest.starts_with("password[") {
            let open = at + "password[".len();
            out.push_str(&line[at..open]);
            out.push_str(HIDDEN);
            at = line[open..]
                .find(']')
                .map_or(line.len(), |close| open + close);
            continue;
        }
        if rest.starts_with("?p=") {
            let open = at + "?p=".len();
            out.push_str(&line[at..open]);
            out.push_str(HIDDEN);
            at = line[open..]
                .find(|c: char| c == '?' || c.is_whitespace())
                .map_or(line.len(), |close| open + close);
            continue;
        }
        let next = line[at..].chars().next().map_or(1, char::len_utf8);
        out.push_str(&line[at..at + next]);
        at += next;
    }
    Cow::Owned(out)
}

/// Every key Jagex documents; world rules such as PvP and difficulty live in the save instead.
pub const CATALOGUE: &[Known] = &[
    Known {
        key: "OwnerId",
        label: "Owner's Player ID",
        help: "Copy it from the bottom of the Settings menu in the game. The owner can ban, unban and change the world. The server will not start without it.",
        group: "Owner",
        default: "",
        kind: Kind::Text,
    },
    Known {
        key: "ServerName",
        label: "Server name",
        help: "Shown under Created By. Leave empty to use the server's name here.",
        group: "Listing",
        default: "",
        kind: Kind::Text,
    },
    Known {
        key: "DefaultWorldName",
        label: "World name",
        help: "Players search for it under Worlds, Public, and it is the save file's name. Case matters.",
        group: "Listing",
        default: "",
        kind: Kind::Text,
    },
    Known {
        key: "PlatformPolicy",
        label: "Who can join",
        help: "Crossplay lets every platform in; the others keep the world to one.",
        group: "Listing",
        default: "Crossplay",
        kind: Kind::Choice {
            options: &["Crossplay", "PC", "PlayStation", "Xbox", "Nintendo"],
        },
    },
    Known {
        key: "WorldPassword",
        label: "World password",
        help: "Leave empty for an open world. It replaces any password the save itself holds.",
        group: "Access",
        default: "",
        kind: Kind::Text,
    },
    Known {
        key: "AdminPassword",
        label: "Admin password",
        help: "Typed in the game under Settings, Server Management, it lets a player kick and ban. Change it and restart to take admin away.",
        group: "Access",
        default: "",
        kind: Kind::Text,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    const GENERATED: &str = ";METADATA=(Diff=true, UseCommands=true)\r\n[/Script/Dominion.DedicatedServerSettings]\r\nAdminPassword=0H9Q8K8DX6M2KYPJ\r\nOwnerId=\r\nServerGuid=6E8B93DDB72D4F9E95D04A5E31ED22B8\r\nServerName=Server-3955\r\nWorldPassword=\r\nDefaultWorldName=World-75058\r\nKnownPlayerList=(UserId=0123456789abcdef0123456789abcdef,UserName=\"Wanderer\",Privileges=(PrivilegeMask=14),LastAdminPassword=\"\",bIsBanned=False)\r\n\r\n[SectionsToSave]\r\nbCanSaveAllSections=true\r\n";

    #[test]
    fn changes_one_key_and_keeps_the_rest_of_the_file() {
        let mut ini = Ini::parse(GENERATED);
        assert_eq!(
            ini.get(SECTION, "ServerName").as_deref(),
            Some("Server-3955")
        );
        ini.set(SECTION, "OwnerId", "0123456789abcdef0123456789abcdef");
        ini.set(SECTION, "PlatformPolicy", "PC");
        let text = ini.render();
        assert!(text.contains("OwnerId=0123456789abcdef0123456789abcdef\r\n"));
        assert!(text.contains("ServerGuid=6E8B93DDB72D4F9E95D04A5E31ED22B8\r\n"));
        assert!(text.contains("KnownPlayerList=(UserId="));
        assert!(text.starts_with(";METADATA="));
        let policy = text.find("PlatformPolicy=PC").unwrap();
        assert!(policy < text.find("[SectionsToSave]").unwrap());
        assert_eq!(ini.entries("SectionsToSave").len(), 1);
    }

    #[test]
    fn a_file_that_is_not_there_yet_gets_the_section() {
        let mut ini = Ini::default();
        set(&mut ini, "OwnerId", "abc");
        assert_eq!(ini.render(), format!("[{SECTION}]\nOwnerId=abc\n"));
        set(&mut ini, "OwnerId", " ");
        assert!(ini.get(SECTION, "OwnerId").is_none());
    }

    #[test]
    fn the_player_id_is_thirty_two_hex_digits() {
        assert_eq!(
            check_owner(" 0123456789ABCDEF0123456789ABCDEF ").unwrap(),
            "0123456789abcdef0123456789abcdef"
        );
        assert!(check_owner("not-an-id").is_err());
        assert!(check_owner("").is_err());
    }

    #[test]
    fn a_world_name_is_safe_as_a_file_name() {
        assert_eq!(world_name_from("Judd's Dragon Den!"), "Judds Dragon Den");
        assert_eq!(world_name_from("???"), "World");
        assert!(check_world("My World_2").is_ok());
        assert!(check_world("../escape").is_err());
    }

    #[test]
    fn both_ports_follow_the_game_port() {
        let command = start_command(
            Path::new("servers/one"),
            Path::new("/config/steamcmd/home"),
            7777,
        );
        let words = shell_words::split(&command).unwrap();
        assert!(words.contains(&"RSDragonwilds".to_string()));
        assert!(words.contains(&"-Port=7777".to_string()));
        assert!(words.contains(&"-BeaconPort=8888".to_string()));
        assert_eq!(beacon_in(&command), Some(8888));

        let moved = with_port(&command, 7800).unwrap();
        assert!(moved.contains("-Port=7800"));
        assert_eq!(beacon_in(&moved), Some(8911));
        assert!(with_port("./server", 1).is_none());
    }

    #[test]
    fn reads_who_joined_and_who_left() {
        let joined = "[2026.08.09-19.37.07:655][120]LogDomMatcherSession: Player ADDED to session [0123456789abcdef0123456789abcdef]-[Wanderer]";
        assert_eq!(
            arrival(joined),
            Some(("0123456789abcdef0123456789abcdef".into(), "Wanderer".into()))
        );
        let left = "LogDomMatcherSession: Player Removed from session [0123456789abcdef0123456789abcdef]-[Two Words]";
        assert_eq!(
            departure(left).map(|(_, name)| name).as_deref(),
            Some("Two Words")
        );
        assert_eq!(arrival(left), None);
        assert_eq!(departure("LogNet: Join succeeded: Wanderer"), None);
    }

    #[test]
    fn passwords_never_reach_the_console() {
        assert_eq!(
            redact("LogDomServerConfig: Dedicated Server WorldPassword: hunter2"),
            "LogDomServerConfig: Dedicated Server WorldPassword: ********"
        );
        assert_eq!(
            redact("LogX: world [A] Password[hunter2] Owner[x]"),
            "LogX: world [A] Password[********] Owner[x]"
        );
        assert_eq!(
            redact("LogNet: Login request: ?p=aHVudGVyMg==?pf=PC?Name=W userId: x"),
            "LogNet: Login request: ?p=********?pf=PC?Name=W userId: x"
        );
        assert_eq!(
            redact("LogNet: PreLogin failure: PLogPassword"),
            "LogNet: PreLogin failure: PLogPassword"
        );
        assert!(matches!(redact("LogInit: Build: 1"), Cow::Borrowed(_)));
    }

    #[tokio::test]
    async fn the_admin_password_survives_the_game_rewriting_its_file() {
        let directory = std::env::temp_dir().join("crustation-dragonwilds-admin");
        tokio::fs::remove_dir_all(&directory).await.ok();
        configure(
            &directory,
            "Den",
            &[("AdminPassword".into(), "s3cret".into())],
        )
        .await
        .unwrap();

        // What the game writes back after a start: everything but the admin password.
        let mut rewritten = load(&directory).await.unwrap().unwrap();
        rewritten.remove(SECTION, "AdminPassword");
        save(&directory, &rewritten).await.unwrap();

        let shown = load_with_admin(&directory).await.unwrap().unwrap();
        assert_eq!(
            shown.get(SECTION, "AdminPassword").as_deref(),
            Some("s3cret")
        );
        before_start(&directory).await.unwrap();
        let written = load(&directory).await.unwrap().unwrap();
        assert_eq!(
            written.get(SECTION, "AdminPassword").as_deref(),
            Some("s3cret")
        );

        let mut cleared = shown.clone();
        set(&mut cleared, "AdminPassword", "");
        save_with_admin(&directory, &cleared).await.unwrap();
        let shown = load_with_admin(&directory).await.unwrap().unwrap();
        assert!(shown.get(SECTION, "AdminPassword").is_none());
        tokio::fs::remove_dir_all(&directory).await.ok();
    }

    #[test]
    fn verbose_lines_are_noise_and_players_are_not() {
        assert!(is_noise(
            "[2026.10.16-23.30.17:148][ 50]LogRedpointEOS: Verbose: [LogEOSHTTP] Finished successful HTTP request for operation HeartbeatSession"
        ));
        assert!(!is_noise(
            "LogDomMatcherSession: Player ADDED to session [0123456789abcdef0123456789abcdef]-[Wanderer]"
        ));
        assert!(!is_noise(
            "LogDomGameMode: World settings beacon listening on port 8888"
        ));
    }

    #[test]
    fn the_managed_keys_include_their_array_forms() {
        assert!(is_managed("KnownPlayerList"));
        assert!(is_managed("+KnownPlayerList"));
        assert!(is_managed("ServerGuid"));
        assert!(!is_managed("ServerName"));
    }
}
