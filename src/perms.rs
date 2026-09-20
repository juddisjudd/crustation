use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fmt;
use std::str::FromStr;

/// Things a user may do panel-wide.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Global {
    CreateServer,
    ManageUsers,
    ManageRoles,
    Admin,
}

/// Things a user may do to one game server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Server {
    Commands,
    Console,
    Logs,
    Schedules,
    Backups,
    Files,
    Config,
    Players,
}

impl Global {
    pub const ALL: [Global; 4] = [
        Global::CreateServer,
        Global::ManageUsers,
        Global::ManageRoles,
        Global::Admin,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Global::CreateServer => "CREATE_SERVER",
            Global::ManageUsers => "MANAGE_USERS",
            Global::ManageRoles => "MANAGE_ROLES",
            Global::Admin => "ADMIN",
        }
    }
}

impl Server {
    pub const ALL: [Server; 8] = [
        Server::Commands,
        Server::Console,
        Server::Logs,
        Server::Schedules,
        Server::Backups,
        Server::Files,
        Server::Config,
        Server::Players,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Server::Commands => "COMMANDS",
            Server::Console => "CONSOLE",
            Server::Logs => "LOGS",
            Server::Schedules => "SCHEDULES",
            Server::Backups => "BACKUPS",
            Server::Files => "FILES",
            Server::Config => "CONFIG",
            Server::Players => "PLAYERS",
        }
    }
}

impl FromStr for Global {
    type Err = ();
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Global::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == value)
            .ok_or(())
    }
}

impl FromStr for Server {
    type Err = ();
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Server::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == value)
            .ok_or(())
    }
}

impl fmt::Display for Global {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl fmt::Display for Server {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Permissions are stored as comma-separated names so they stay readable in the database.
pub fn parse_global(value: &str) -> BTreeSet<Global> {
    value
        .split(',')
        .filter_map(|name| Global::from_str(name.trim()).ok())
        .collect()
}

pub fn parse_server(value: &str) -> BTreeSet<Server> {
    value
        .split(',')
        .filter_map(|name| Server::from_str(name.trim()).ok())
        .collect()
}

#[allow(dead_code, reason = "used when saving roles")]
pub fn join<T: fmt::Display>(values: impl IntoIterator<Item = T>) -> String {
    values
        .into_iter()
        .map(|value| value.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_known_names_and_ignores_junk() {
        let parsed = parse_server("FILES, CONSOLE ,NOPE,");
        assert!(parsed.contains(&Server::Files));
        assert!(parsed.contains(&Server::Console));
        assert_eq!(parsed.len(), 2);
    }

    #[test]
    fn round_trips_through_storage() {
        let stored = join([Global::CreateServer, Global::ManageUsers]);
        assert_eq!(parse_global(&stored).len(), 2);
    }

    #[test]
    fn empty_string_grants_nothing() {
        assert!(parse_server("").is_empty());
        assert!(parse_global("").is_empty());
    }
}
