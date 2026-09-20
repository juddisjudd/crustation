use std::collections::HashSet;

use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::IntoResponse;
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::auth::Identity;
use crate::events::Topic;
use crate::perms::Server as ServerPerm;
use crate::state::AppState;

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Incoming {
    Subscribe { topics: Vec<String> },
    Unsubscribe { topics: Vec<String> },
    Ping,
}

pub async fn handler(
    identity: Identity,
    State(state): State<AppState>,
    upgrade: WebSocketUpgrade,
) -> impl IntoResponse {
    upgrade.on_upgrade(move |socket| run(socket, state, identity))
}

async fn run(socket: WebSocket, state: AppState, identity: Identity) {
    let (mut sink, mut stream) = socket.split();
    let mut events = state.events.subscribe();
    let mut topics: HashSet<String> = HashSet::new();

    loop {
        tokio::select! {
            incoming = stream.next() => {
                let Some(Ok(message)) = incoming else { break };
                match message {
                    Message::Text(text) => {
                        match serde_json::from_str::<Incoming>(&text) {
                            Ok(Incoming::Subscribe { topics: wanted }) => {
                                for name in wanted {
                                    let Some(topic) = Topic::parse(&name) else {
                                        let _ = sink.send(error_frame("UNKNOWN_TOPIC", &name)).await;
                                        continue;
                                    };
                                    if allowed(&state, &identity, &topic).await {
                                        topics.insert(topic.as_string());
                                    } else {
                                        let _ = sink.send(error_frame("FORBIDDEN", &name)).await;
                                    }
                                }
                            }
                            Ok(Incoming::Unsubscribe { topics: unwanted }) => {
                                for name in unwanted {
                                    topics.remove(&name);
                                }
                            }
                            Ok(Incoming::Ping) => {
                                let _ = sink.send(Message::Text(json!({"type":"pong"}).to_string().into())).await;
                            }
                            Err(_) => {
                                let _ = sink.send(error_frame("BAD_MESSAGE", "")).await;
                            }
                        }
                    }
                    Message::Close(_) => break,
                    _ => {}
                }
            }
            event = events.recv() => {
                match event {
                    Ok(event) => {
                        if !topics.contains(&event.topic) {
                            continue;
                        }
                        // Permissions can change while a socket is open, so check per event.
                        if let Some(server_id) = event.server_id {
                            if !can_see_server(&state, &identity, server_id).await {
                                continue;
                            }
                        }
                        let frame = json!({
                            "type": "event",
                            "topic": event.topic,
                            "event": event.event,
                            "data": event.data,
                        });
                        if sink.send(Message::Text(frame.to_string().into())).await.is_err() {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(missed)) => {
                        tracing::debug!(missed, "websocket fell behind");
                    }
                    Err(_) => break,
                }
            }
        }
    }
}

async fn allowed(state: &AppState, identity: &Identity, topic: &Topic) -> bool {
    match topic {
        Topic::Servers | Topic::Panel => true,
        Topic::Server(id) => can_see_server(state, identity, *id).await,
        Topic::Console(id) => identity
            .server_permissions(&state.db, *id)
            .await
            .map(|granted| granted.contains(&ServerPerm::Console))
            .unwrap_or(false),
    }
}

async fn can_see_server(state: &AppState, identity: &Identity, id: Uuid) -> bool {
    identity
        .server_permissions(&state.db, id)
        .await
        .map(|granted| !granted.is_empty())
        .unwrap_or(false)
}

fn error_frame(code: &str, detail: &str) -> Message {
    Message::Text(
        json!({ "type": "error", "error": code, "message": detail })
            .to_string()
            .into(),
    )
}
