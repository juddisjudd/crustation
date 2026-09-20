//! The line between the panel and a Bedrock server.
//!
//! Bedrock has no RCON and writes no chat to its console, so a panel can only
//! watch it play dead. The add-on the panel installs closes that: a behaviour
//! pack that posts what happened and asks what to run, on a short timer. Every
//! other edition gets the same things over RCON, so nothing above this cares
//! which way the answer arrived.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use uuid::Uuid;

/// How long after its last word a server is still counted as connected. Three
/// missed polls, so one slow tick does not make the interface flicker.
const QUIET: chrono::Duration = chrono::Duration::seconds(6);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Spot {
    pub name: String,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub dimension: String,
}

/// Something the add-on saw happen.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Happening {
    Chat {
        player: String,
        message: String,
    },
    Join {
        player: String,
    },
    Leave {
        player: String,
    },
    Death {
        player: String,
        message: String,
    },
    /// Anything the add-on wants in the console without a shape of its own.
    Note {
        message: String,
    },
}

impl Happening {
    /// The console line this becomes, written the way the Java server writes
    /// the same thing, so everything that reads the console already suits it.
    pub fn as_console_line(&self) -> String {
        match self {
            Happening::Chat { player, message } => format!("<{player}> {message}"),
            Happening::Join { player } => format!("{player} joined the game"),
            Happening::Leave { player } => format!("{player} left the game"),
            Happening::Death { message, .. } => message.clone(),
            Happening::Note { message } => message.clone(),
        }
    }

    pub fn player(&self) -> Option<&str> {
        match self {
            Happening::Chat { player, .. }
            | Happening::Join { player }
            | Happening::Leave { player }
            | Happening::Death { player, .. } => Some(player),
            Happening::Note { .. } => None,
        }
    }
}

#[derive(Default)]
struct Link {
    last_seen: Option<DateTime<Utc>>,
    players: Vec<Spot>,
    /// Every item the running server knows, add-ons included. Asked for once
    /// and kept, since it only changes when the packs or the version do.
    items: Option<Vec<String>>,
}

/// Every server with the add-on installed, and what it last said.
#[derive(Default)]
pub struct Bridges {
    links: RwLock<HashMap<Uuid, Link>>,
}

impl Bridges {
    /// The add-on checking in with what it has seen.
    pub async fn arrived(&self, id: Uuid, players: Vec<Spot>) {
        let mut links = self.links.write().await;
        let link = links.entry(id).or_default();
        link.last_seen = Some(Utc::now());
        link.players = players;
    }

    /// The add-on answering the panel's standing request for the item list.
    pub async fn took_items(&self, id: Uuid, items: Vec<String>) {
        self.links.write().await.entry(id).or_default().items = Some(items);
    }

    /// What to tell the add-on on its next check-in. Unlike where people are,
    /// a list held from before the server stopped is still true, so this only
    /// asks again when the panel has nothing at all.
    pub async fn wants_items(&self, id: Uuid) -> bool {
        !self
            .links
            .read()
            .await
            .get(&id)
            .is_some_and(|link| link.items.is_some())
    }

    pub async fn items(&self, id: Uuid) -> Option<Vec<String>> {
        self.links
            .read()
            .await
            .get(&id)
            .and_then(|link| link.items.clone())
    }

    pub async fn players(&self, id: Uuid) -> Vec<Spot> {
        match self.connected(id).await {
            true => self
                .links
                .read()
                .await
                .get(&id)
                .map(|link| link.players.clone())
                .unwrap_or_default(),
            // Where everybody stood ten minutes ago is not where they are.
            false => Vec::new(),
        }
    }

    /// Whether the add-on has been heard from recently enough to trust.
    pub async fn connected(&self, id: Uuid) -> bool {
        let links = self.links.read().await;
        links
            .get(&id)
            .and_then(|link| link.last_seen)
            .is_some_and(|at| Utc::now() - at < QUIET)
    }

    pub async fn last_seen(&self, id: Uuid) -> Option<DateTime<Utc>> {
        self.links
            .read()
            .await
            .get(&id)
            .and_then(|link| link.last_seen)
    }

    /// Everything about a server goes when the server does.
    pub async fn forget(&self, id: Uuid) {
        self.links.write().await.remove(&id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn somebody() -> Spot {
        Spot {
            name: "ohitsjudd".into(),
            x: 1.0,
            y: 64.0,
            z: -2.0,
            dimension: "minecraft:overworld".into(),
        }
    }

    #[tokio::test]
    async fn a_server_that_has_never_spoken_is_not_connected() {
        let bridges = Bridges::default();
        assert!(!bridges.connected(Uuid::new_v4()).await);
        assert!(bridges.players(Uuid::new_v4()).await.is_empty());
    }

    #[tokio::test]
    async fn checking_in_makes_it_connected_and_carries_the_players() {
        let bridges = Bridges::default();
        let id = Uuid::new_v4();
        bridges.arrived(id, vec![somebody()]).await;
        assert!(bridges.connected(id).await);
        assert_eq!(bridges.players(id).await.len(), 1);
        assert!(bridges.last_seen(id).await.is_some());
    }

    #[tokio::test]
    async fn a_later_check_in_replaces_the_last_one_rather_than_adding_to_it() {
        let bridges = Bridges::default();
        let id = Uuid::new_v4();
        bridges.arrived(id, vec![somebody(), somebody()]).await;
        bridges.arrived(id, vec![somebody()]).await;
        assert_eq!(bridges.players(id).await.len(), 1);
    }

    #[tokio::test]
    async fn the_item_list_is_asked_for_until_it_arrives_and_then_left_alone() {
        let bridges = Bridges::default();
        let id = Uuid::new_v4();
        assert!(bridges.wants_items(id).await);
        assert!(bridges.items(id).await.is_none());

        bridges.took_items(id, vec!["diamond".into()]).await;
        assert!(!bridges.wants_items(id).await);
        assert_eq!(bridges.items(id).await.unwrap(), vec!["diamond"]);
    }

    #[tokio::test]
    async fn a_server_that_has_gone_quiet_still_knows_its_items() {
        let bridges = Bridges::default();
        let id = Uuid::new_v4();
        bridges.took_items(id, vec!["diamond".into()]).await;
        // Never checked in, so not connected, and where people are is unknown.
        assert!(!bridges.connected(id).await);
        assert!(bridges.players(id).await.is_empty());
        // The items it holds did not stop being true.
        assert_eq!(bridges.items(id).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn forgetting_a_server_clears_it() {
        let bridges = Bridges::default();
        let id = Uuid::new_v4();
        bridges.arrived(id, vec![somebody()]).await;
        bridges.forget(id).await;
        assert!(!bridges.connected(id).await);
        assert!(bridges.players(id).await.is_empty());
    }

    #[test]
    fn a_happening_reads_like_the_java_server_writing_the_same_thing() {
        let chat = Happening::Chat {
            player: "Notch".into(),
            message: "hello".into(),
        };
        assert_eq!(chat.as_console_line(), "<Notch> hello");
        assert_eq!(chat.player(), Some("Notch"));

        let join = Happening::Join {
            player: "Notch".into(),
        };
        assert_eq!(join.as_console_line(), "Notch joined the game");

        let leave = Happening::Leave {
            player: "Notch".into(),
        };
        assert_eq!(leave.as_console_line(), "Notch left the game");

        let note = Happening::Note {
            message: "the add-on woke up".into(),
        };
        assert_eq!(note.as_console_line(), "the add-on woke up");
        assert_eq!(note.player(), None);
    }

    #[test]
    fn what_the_add_on_sends_is_what_the_panel_reads() {
        let sent = r#"{"kind":"chat","player":"ohitsjudd","message":"hi"}"#;
        let parsed: Happening = serde_json::from_str(sent).expect("parse");
        assert_eq!(parsed.as_console_line(), "<ohitsjudd> hi");

        let spot = r#"{"name":"a","x":1.5,"y":64.0,"z":-2.5,"dimension":"minecraft:the_nether"}"#;
        let parsed: Spot = serde_json::from_str(spot).expect("parse");
        assert_eq!(parsed.dimension, "minecraft:the_nether");
        assert_eq!(parsed.z, -2.5);
    }
}
