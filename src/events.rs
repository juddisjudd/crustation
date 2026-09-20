use serde::Serialize;
use serde_json::{Value, json};
use tokio::sync::broadcast;
use uuid::Uuid;

/// What a client can subscribe to. Topics are strings on the wire; this keeps the
/// producer side honest about which ones exist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Topic {
    /// Every server the subscriber may see: state changes and stats.
    Servers,
    /// One server: state, stats, players, install and backup progress.
    Server(Uuid),
    /// One server's console output. Needs the CONSOLE permission.
    Console(Uuid),
    /// Host stats, notifications, audit entries.
    Panel,
}

impl Topic {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "servers" => Some(Topic::Servers),
            "panel" => Some(Topic::Panel),
            other => {
                let rest = other.strip_prefix("server:")?;
                match rest.split_once(':') {
                    Some((id, "console")) => Uuid::parse_str(id).ok().map(Topic::Console),
                    None => Uuid::parse_str(rest).ok().map(Topic::Server),
                    _ => None,
                }
            }
        }
    }

    pub fn as_string(&self) -> String {
        match self {
            Topic::Servers => "servers".into(),
            Topic::Server(id) => format!("server:{id}"),
            Topic::Console(id) => format!("server:{id}:console"),
            Topic::Panel => "panel".into(),
        }
    }

    /// The server this topic is about, if any, so delivery can check permissions.
    pub fn server_id(&self) -> Option<Uuid> {
        match self {
            Topic::Server(id) | Topic::Console(id) => Some(*id),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Event {
    pub topic: String,
    pub event: String,
    pub data: Value,
    /// Set when the event concerns one server, for permission checks on delivery.
    #[serde(skip)]
    pub server_id: Option<Uuid>,
}

#[derive(Clone)]
pub struct Events {
    sender: broadcast::Sender<Event>,
}

impl Events {
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(1024);
        Self { sender }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.sender.subscribe()
    }

    pub fn publish(&self, topic: Topic, event: &str, data: Value) {
        let message = Event {
            topic: topic.as_string(),
            event: event.to_string(),
            data,
            server_id: topic.server_id(),
        };
        // An error only means nobody is listening.
        let _ = self.sender.send(message);
    }

    pub fn server_state(&self, server_id: Uuid, state: &str, flags: Value) {
        let data = json!({ "server_id": server_id, "state": state, "flags": flags });
        self.publish(Topic::Server(server_id), "state", data.clone());
        self.publish(Topic::Servers, "state", data);
    }

    pub fn server_stats(&self, server_id: Uuid, stats: Value) {
        self.publish(Topic::Server(server_id), "stats", stats.clone());
        self.publish(Topic::Servers, "stats", stats);
    }

    pub fn console_line(&self, server_id: Uuid, line: Value) {
        self.publish(Topic::Console(server_id), "line", line);
    }

    pub fn console_cleared(&self, server_id: Uuid) {
        self.publish(Topic::Console(server_id), "cleared", json!({}));
    }

    pub fn notify(&self, level: &str, message: impl Into<String>) {
        self.publish(
            Topic::Panel,
            "notification",
            json!({ "level": level, "message": message.into() }),
        );
    }
}

impl Default for Events {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn topics_round_trip() {
        let id = Uuid::new_v4();
        for topic in [
            Topic::Servers,
            Topic::Panel,
            Topic::Server(id),
            Topic::Console(id),
        ] {
            let text = topic.as_string();
            assert_eq!(Topic::parse(&text), Some(topic));
        }
    }

    #[test]
    fn rejects_nonsense_topics() {
        assert!(Topic::parse("server:not-a-uuid").is_none());
        assert!(Topic::parse("server:").is_none());
        assert!(Topic::parse("everything").is_none());
    }

    #[test]
    fn server_topics_carry_their_id() {
        let id = Uuid::new_v4();
        assert_eq!(Topic::Console(id).server_id(), Some(id));
        assert_eq!(Topic::Servers.server_id(), None);
    }
}
