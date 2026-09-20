use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use serde::Serialize;
use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpStream, UdpSocket};
use tokio::sync::Mutex;
use uuid::Uuid;

/// What a server says about itself when asked over the wire. This is the same
/// question a player's server list asks, so it answers for servers the panel did
/// not start too.
#[derive(Debug, Clone, Serialize)]
pub struct Status {
    pub motd: String,
    pub version: String,
    pub players_online: i64,
    pub players_max: i64,
    pub latency_ms: i64,
    /// Who the server named. Java sends a short sample, usually a dozen at most,
    /// and sends none at all when `hide-online-players` is on. Bedrock's pong
    /// carries no names, so this is empty there.
    pub sample: Vec<Player>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Player {
    pub name: String,
    pub uuid: Option<String>,
}

const TIMEOUT: Duration = Duration::from_secs(3);

/// The last answer from each server. A server that did not reply drops out, so
/// what is here is what was true at the most recent sample.
#[derive(Clone, Default)]
pub struct Statuses(Arc<Mutex<HashMap<Uuid, Status>>>);

impl Statuses {
    pub async fn record(&self, id: Uuid, status: Option<Status>) {
        let mut guard = self.0.lock().await;
        match status {
            Some(status) => guard.insert(id, status),
            None => guard.remove(&id),
        };
    }

    pub async fn get(&self, id: Uuid) -> Option<Status> {
        self.0.lock().await.get(&id).cloned()
    }

    pub async fn forget(&self, id: Uuid) {
        self.0.lock().await.remove(&id);
    }
}

/// A server bound to every interface is reached on the loopback, not on 0.0.0.0.
pub fn reachable_host(host: &str) -> &str {
    match host.trim() {
        "" | "0.0.0.0" | "::" | "[::]" | "*" => "127.0.0.1",
        other => other,
    }
}

pub async fn query(kind: &str, host: &str, port: u16) -> Result<Status> {
    let host = reachable_host(host);
    match kind {
        "minecraft_bedrock" => tokio::time::timeout(TIMEOUT, bedrock(host, port))
            .await
            .context("the server did not answer in time")?,
        _ => tokio::time::timeout(TIMEOUT, java(host, port))
            .await
            .context("the server did not answer in time")?,
    }
}

// Java: the Server List Ping handshake, the same one the client's server list uses.
// https://minecraft.wiki/w/Java_Edition_protocol

async fn java(host: &str, port: u16) -> Result<Status> {
    let started = Instant::now();
    let mut stream = TcpStream::connect((host, port))
        .await
        .with_context(|| format!("connecting to {host}:{port}"))?;
    stream.set_nodelay(true).ok();

    let mut handshake = Vec::new();
    handshake.push(0x00); // packet id: handshake
    write_varint(&mut handshake, -1); // protocol version: asking, not joining
    write_string(&mut handshake, host);
    handshake.extend_from_slice(&port.to_be_bytes());
    write_varint(&mut handshake, 1); // next state: status

    let mut framed = Vec::new();
    write_varint(&mut framed, handshake.len() as i32);
    framed.extend_from_slice(&handshake);
    // The empty status request rides along in the same write.
    write_varint(&mut framed, 1);
    framed.push(0x00);
    stream.write_all(&framed).await?;
    stream.flush().await?;

    let length = read_varint(&mut stream)
        .await
        .context("reading the reply")?;
    if !(0..=(1 << 21)).contains(&length) {
        bail!("the reply was not a status packet");
    }
    let mut body = vec![0u8; length as usize];
    stream.read_exact(&mut body).await?;

    let mut cursor = &body[..];
    let packet = take_varint(&mut cursor)?;
    if packet != 0x00 {
        bail!("the server answered with packet {packet:#x}, not a status");
    }
    let json = take_string(&mut cursor)?;
    let latency_ms = started.elapsed().as_millis() as i64;

    let parsed: Value = serde_json::from_str(&json).context("the status was not JSON")?;
    Ok(Status {
        motd: chat_to_text(&parsed["description"]),
        version: parsed["version"]["name"].as_str().unwrap_or("").to_string(),
        players_online: parsed["players"]["online"].as_i64().unwrap_or(0),
        players_max: parsed["players"]["max"].as_i64().unwrap_or(0),
        latency_ms,
        sample: sample_from(&parsed["players"]["sample"]),
    })
}

/// The short list of names Java attaches to a status, when it attaches one.
fn sample_from(value: &Value) -> Vec<Player> {
    value
        .as_array()
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| {
                    let name = entry["name"].as_str()?.trim().to_string();
                    if name.is_empty() {
                        return None;
                    }
                    Some(Player {
                        name,
                        uuid: entry["id"].as_str().map(str::to_string),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// A description is either a plain string or a chat component with nested parts.
fn chat_to_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Array(parts) => parts.iter().map(chat_to_text).collect(),
        Value::Object(map) => {
            let mut out = String::new();
            if let Some(Value::String(text)) = map.get("text") {
                out.push_str(text);
            }
            if let Some(extra) = map.get("extra") {
                out.push_str(&chat_to_text(extra));
            }
            // Pre-1.13 servers put the whole thing here.
            if out.is_empty()
                && let Some(Value::String(text)) = map.get("translate")
            {
                out.push_str(text);
            }
            out
        }
        _ => String::new(),
    }
}

// Bedrock: RakNet's unconnected ping, which answers with a semicolon-separated line.
// https://minecraft.wiki/w/RakNet

const RAKNET_MAGIC: [u8; 16] = [
    0x00, 0xff, 0xff, 0x00, 0xfe, 0xfe, 0xfe, 0xfe, 0xfd, 0xfd, 0xfd, 0xfd, 0x12, 0x34, 0x56, 0x78,
];

async fn bedrock(host: &str, port: u16) -> Result<Status> {
    let socket = UdpSocket::bind("0.0.0.0:0").await?;
    socket
        .connect((host, port))
        .await
        .with_context(|| format!("reaching {host}:{port}"))?;

    let mut request = Vec::with_capacity(33);
    request.push(0x01); // unconnected ping
    request.extend_from_slice(&0i64.to_be_bytes()); // our clock, echoed back
    request.extend_from_slice(&RAKNET_MAGIC);
    request.extend_from_slice(&0i64.to_be_bytes()); // client guid

    let started = Instant::now();
    socket.send(&request).await?;

    let mut buffer = [0u8; 1500];
    let read = socket.recv(&mut buffer).await?;
    let latency_ms = started.elapsed().as_millis() as i64;

    let reply = &buffer[..read];
    if reply.first() != Some(&0x1c) {
        bail!("the server did not answer with a pong");
    }
    // id, timestamp, server guid, magic, then a length-prefixed line.
    if reply.len() < 35 {
        bail!("the pong was too short to read");
    }
    let length = u16::from_be_bytes([reply[33], reply[34]]) as usize;
    let end = (35 + length).min(reply.len());
    let line = String::from_utf8_lossy(&reply[35..end]).to_string();

    parse_bedrock(&line, latency_ms)
}

/// `MCPE;motd;protocol;version;online;max;guid;level;gamemode;…`
fn parse_bedrock(line: &str, latency_ms: i64) -> Result<Status> {
    let parts: Vec<&str> = line.split(';').collect();
    if parts.len() < 6 {
        bail!("the pong did not describe a server");
    }
    Ok(Status {
        motd: parts[1].to_string(),
        version: parts[3].to_string(),
        players_online: parts[4].parse().unwrap_or(0),
        players_max: parts[5].parse().unwrap_or(0),
        latency_ms,
        // A pong carries counts, never names.
        sample: Vec::new(),
    })
}

// Varints and strings, the way the Java protocol writes them.

fn write_varint(out: &mut Vec<u8>, mut value: i32) {
    loop {
        let mut byte = (value & 0x7f) as u8;
        value = ((value as u32) >> 7) as i32;
        if value != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if value == 0 {
            return;
        }
    }
}

fn write_string(out: &mut Vec<u8>, value: &str) {
    write_varint(out, value.len() as i32);
    out.extend_from_slice(value.as_bytes());
}

async fn read_varint(stream: &mut TcpStream) -> Result<i32> {
    let mut value = 0i32;
    for shift in 0..5 {
        let mut byte = [0u8; 1];
        stream.read_exact(&mut byte).await?;
        value |= ((byte[0] & 0x7f) as i32) << (shift * 7);
        if byte[0] & 0x80 == 0 {
            return Ok(value);
        }
    }
    bail!("a varint ran past five bytes")
}

fn take_varint(cursor: &mut &[u8]) -> Result<i32> {
    let mut value = 0i32;
    for shift in 0..5 {
        let (first, rest) = cursor.split_first().context("the packet ended early")?;
        *cursor = rest;
        value |= ((first & 0x7f) as i32) << (shift * 7);
        if first & 0x80 == 0 {
            return Ok(value);
        }
    }
    bail!("a varint ran past five bytes")
}

fn take_string(cursor: &mut &[u8]) -> Result<String> {
    let length = take_varint(cursor)? as usize;
    if cursor.len() < length {
        bail!("the packet promised more text than it carried");
    }
    let (text, rest) = cursor.split_at(length);
    *cursor = rest;
    Ok(String::from_utf8_lossy(text).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_the_varints_the_protocol_expects() {
        let cases: [(i32, &[u8]); 6] = [
            (0, &[0x00]),
            (1, &[0x01]),
            (127, &[0x7f]),
            (128, &[0x80, 0x01]),
            (255, &[0xff, 0x01]),
            (-1, &[0xff, 0xff, 0xff, 0xff, 0x0f]),
        ];
        for (value, expected) in cases {
            let mut out = Vec::new();
            write_varint(&mut out, value);
            assert_eq!(out, expected, "writing {value}");

            let mut cursor = &out[..];
            assert_eq!(take_varint(&mut cursor).expect("reads back"), value);
        }
    }

    #[test]
    fn reads_a_length_prefixed_string() {
        let mut out = Vec::new();
        write_string(&mut out, "Hello");
        let mut cursor = &out[..];
        assert_eq!(take_string(&mut cursor).expect("reads"), "Hello");
        assert!(cursor.is_empty());
    }

    #[test]
    fn refuses_a_string_longer_than_the_packet() {
        let packet = [0x40u8, b'a', b'b'];
        let mut cursor = &packet[..];
        assert!(take_string(&mut cursor).is_err());
    }

    #[test]
    fn flattens_a_description_however_it_is_written() {
        assert_eq!(chat_to_text(&serde_json::json!("Plain")), "Plain");
        assert_eq!(
            chat_to_text(&serde_json::json!({"text": "A ", "extra": [{"text": "server"}]})),
            "A server"
        );
        assert_eq!(
            chat_to_text(&serde_json::json!([{"text": "one "}, {"text": "two"}])),
            "one two"
        );
        assert_eq!(chat_to_text(&Value::Null), "");
    }

    #[test]
    fn reads_what_bedrock_says_about_itself() {
        let line = "MCPE;Dedicated Server;800;1.21.51;3;10;1234567890;Bedrock level;Survival;1;19132;19133;";
        let status = parse_bedrock(line, 7).expect("parses");
        assert_eq!(status.motd, "Dedicated Server");
        assert_eq!(status.version, "1.21.51");
        assert_eq!(status.players_online, 3);
        assert_eq!(status.players_max, 10);
        assert_eq!(status.latency_ms, 7);
    }

    #[test]
    fn refuses_a_pong_that_says_nothing() {
        assert!(parse_bedrock("MCPE;only", 1).is_err());
    }

    #[test]
    fn sends_a_ping_to_the_loopback_when_the_server_binds_everything() {
        assert_eq!(reachable_host("0.0.0.0"), "127.0.0.1");
        assert_eq!(reachable_host("::"), "127.0.0.1");
        assert_eq!(reachable_host(""), "127.0.0.1");
        assert_eq!(reachable_host("mc.example.test"), "mc.example.test");
    }
}
