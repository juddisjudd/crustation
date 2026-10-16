//! The games Steam hosts: what SteamCMD is asked for and what each game can be told.

/// One dedicated server installed through SteamCMD.
#[derive(Debug, Clone, Copy)]
pub struct Title {
    pub kind: &'static str,
    pub name: &'static str,
    pub app_id: u32,
    /// Only a Windows build exists, so SteamCMD has to be asked for it and Linux runs it under Wine.
    pub windows_only: bool,
    /// Whether the game reads commands at all, over RCON or its console.
    pub takes_commands: bool,
}

pub const TITLES: [Title; 3] = [
    Title {
        kind: crate::rust_game::KIND,
        name: "Rust",
        app_id: crate::rust_game::APP_ID,
        windows_only: false,
        takes_commands: true,
    },
    Title {
        kind: crate::enshrouded::KIND,
        name: "Enshrouded",
        app_id: crate::enshrouded::APP_ID,
        windows_only: true,
        takes_commands: false,
    },
    Title {
        kind: crate::dragonwilds::KIND,
        name: "RuneScape: Dragonwilds",
        app_id: crate::dragonwilds::APP_ID,
        windows_only: false,
        takes_commands: false,
    },
];

pub fn find(kind: &str) -> Option<&'static Title> {
    TITLES.iter().find(|title| title.kind == kind)
}

pub fn is_steam(kind: &str) -> bool {
    find(kind).is_some()
}

/// Minecraft reads its console and RCON; a Steam title says for itself.
pub fn takes_commands(kind: &str) -> bool {
    find(kind).is_none_or(|title| title.takes_commands)
}

/// The file a Steam title is started from, relative to its folder.
pub fn executable(kind: &str) -> Option<&'static str> {
    match kind {
        crate::rust_game::KIND => Some(crate::rust_game::executable()),
        crate::enshrouded::KIND => Some(crate::enshrouded::executable()),
        crate::dragonwilds::KIND => Some(crate::dragonwilds::executable()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_title_has_a_start_file() {
        for title in TITLES {
            assert!(executable(title.kind).is_some(), "{}", title.kind);
        }
        assert!(find("minecraft_java").is_none());
    }

    #[test]
    fn only_minecraft_and_rust_take_commands() {
        assert!(takes_commands("minecraft_java"));
        assert!(takes_commands("minecraft_bedrock"));
        assert!(takes_commands(crate::rust_game::KIND));
        assert!(!takes_commands(crate::enshrouded::KIND));
        assert!(!takes_commands(crate::dragonwilds::KIND));
    }
}
