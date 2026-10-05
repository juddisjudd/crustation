//! Rust's websocket RCON: a JSON message out, and the reply that carries its identifier back.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use futures::stream::SplitSink;
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::net::TcpStream;
use tokio::sync::{Mutex, broadcast, oneshot};
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

use crate::rcon::Endpoint;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
const COMMAND_TIMEOUT: Duration = Duration::from_secs(10);

type Sink = SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>;
type Waiting = Arc<std::sync::Mutex<HashMap<i64, oneshot::Sender<String>>>>;

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Reply {
    identifier: i64,
    message: String,
    #[serde(rename = "Type", default)]
    kind: String,
}

/// One open socket per listener, since Rust refuses quick reconnects (`rcon.connectioncooldown`).
struct Connection {
    sink: Mutex<Sink>,
    waiting: Waiting,
    alive: Arc<AtomicBool>,
    chat: broadcast::Sender<String>,
}

static OPEN: LazyLock<Mutex<HashMap<String, Arc<Connection>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static NEXT_ID: AtomicI64 = AtomicI64::new(1);

async fn connect(url: &str) -> Result<Arc<Connection>> {
    let (socket, _) = timeout(CONNECT_TIMEOUT, tokio_tungstenite::connect_async(url))
        .await
        .context("RCON did not answer in time")?
        .context("connecting to RCON")?;
    let (sink, mut stream) = socket.split();
    let waiting: Waiting = Arc::default();
    let alive = Arc::new(AtomicBool::new(true));
    let (chat, _) = broadcast::channel(64);

    let (replies, still_up, said) = (waiting.clone(), alive.clone(), chat.clone());
    tokio::spawn(async move {
        while let Some(Ok(frame)) = stream.next().await {
            let Message::Text(text) = frame else {
                continue;
            };
            // Chat and log lines share the socket, under identifiers nobody is waiting on.
            let Ok(reply) = serde_json::from_str::<Reply>(&text) else {
                continue;
            };
            let waiter = replies.lock().expect("lock").remove(&reply.identifier);
            match waiter {
                Some(waiter) => {
                    waiter.send(reply.message).ok();
                }
                None if reply.kind == "Chat" => {
                    said.send(reply.message).ok();
                }
                None => {}
            }
        }
        still_up.store(false, Ordering::SeqCst);
        // Dropping the senders tells everyone still waiting that no answer is coming.
        replies.lock().expect("lock").clear();
    });

    Ok(Arc::new(Connection {
        sink: Mutex::new(sink),
        waiting,
        alive,
        chat,
    }))
}

async fn connection(url: &str) -> Result<Arc<Connection>> {
    let mut open = OPEN.lock().await;
    if let Some(found) = open
        .get(url)
        .filter(|found| found.alive.load(Ordering::SeqCst))
    {
        return Ok(found.clone());
    }
    let fresh = connect(url).await?;
    open.insert(url.to_string(), fresh.clone());
    Ok(fresh)
}

async fn ask(connection: &Connection, command: &str) -> Result<String> {
    let identifier = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let (sender, answer) = oneshot::channel();
    connection
        .waiting
        .lock()
        .expect("lock")
        .insert(identifier, sender);
    let request = serde_json::json!({
        "Identifier": identifier,
        "Message": command,
        "Name": "Crustation",
    });
    if let Err(error) = connection
        .sink
        .lock()
        .await
        .send(Message::text(request.to_string()))
        .await
    {
        connection.alive.store(false, Ordering::SeqCst);
        bail!("RCON closed the connection: {error}");
    }
    match timeout(COMMAND_TIMEOUT, answer).await {
        Ok(Ok(message)) => Ok(message),
        Ok(Err(_)) => bail!("RCON closed the connection"),
        Err(_) => {
            connection.waiting.lock().expect("lock").remove(&identifier);
            bail!("the server did not answer the command")
        }
    }
}

/// Runs one command over the listener's open socket; the server's console never shows it.
pub async fn run(endpoint: &Endpoint, command: &str) -> Result<String> {
    let url = format!("ws://{}/{}", endpoint.address, endpoint.password);
    let open = connection(&url).await?;
    match ask(&open, command).await {
        // A socket the server dropped since, after a restart say, is opened again once.
        Err(_) if !open.alive.load(Ordering::SeqCst) => {
            let reopened = connection(&url).await?;
            ask(&reopened, command).await
        }
        answered => answered,
    }
}

/// Chat as it is said, each message Rust's own JSON, and whether the socket is still up.
pub async fn listen(endpoint: &Endpoint) -> Result<(broadcast::Receiver<String>, Arc<AtomicBool>)> {
    let url = format!("ws://{}/{}", endpoint.address, endpoint.password);
    let open = connection(&url).await?;
    Ok((open.chat.subscribe(), open.alive.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, SocketAddr};
    use tokio::net::TcpListener;

    /// Answers every command on one socket the way Rust does, with a chat message first.
    async fn fake_server() -> (SocketAddr, Arc<AtomicI64>) {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let address = listener.local_addr().unwrap();
        let accepted = Arc::new(AtomicI64::new(0));
        let count = accepted.clone();
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                count.fetch_add(1, Ordering::SeqCst);
                let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
                while let Some(Ok(Message::Text(text))) = socket.next().await {
                    let request: serde_json::Value = serde_json::from_str(&text).unwrap();
                    let chat =
                        serde_json::json!({"Message": "{}", "Identifier": -1, "Type": "Chat"});
                    socket.send(Message::text(chat.to_string())).await.unwrap();
                    let answer = serde_json::json!({
                        "Message": format!("ran {}", request["Message"].as_str().unwrap()),
                        "Identifier": request["Identifier"],
                        "Type": "Generic",
                    });
                    socket
                        .send(Message::text(answer.to_string()))
                        .await
                        .unwrap();
                }
            }
        });
        (address, accepted)
    }

    #[tokio::test]
    async fn answers_each_command_over_one_socket() {
        let (address, accepted) = fake_server().await;
        let endpoint = Endpoint {
            address,
            password: "secret".into(),
        };
        assert_eq!(
            run(&endpoint, "playerlist").await.unwrap(),
            "ran playerlist"
        );
        assert_eq!(run(&endpoint, "bans").await.unwrap(), "ran bans");
        let (one, two) = tokio::join!(run(&endpoint, "a"), run(&endpoint, "b"));
        assert_eq!(
            (one.unwrap(), two.unwrap()),
            ("ran a".into(), "ran b".into())
        );
        assert_eq!(accepted.load(Ordering::SeqCst), 1);
    }
}
