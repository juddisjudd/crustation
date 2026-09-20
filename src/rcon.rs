use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;

use crate::properties::{self, Properties};

/// Source RCON packet types.
const AUTH: i32 = 3;
const EXEC: i32 = 2;
const RESPONSE: i32 = 0;

/// Vanilla answers a command in one packet and does not cap it at the 4 KiB the
/// protocol suggests, so allow a generous body and refuse anything absurd.
const MAX_PACKET: i32 = 2 * 1024 * 1024;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
const COMMAND_TIMEOUT: Duration = Duration::from_secs(10);
/// How long to keep reading after the first packet, for output split across several.
const CONTINUATION_TIMEOUT: Duration = Duration::from_millis(200);

/// Where a server's RCON listener is, and the password for it.
pub struct Endpoint {
    pub address: SocketAddr,
    pub password: String,
}

/// What `server.properties` says about RCON, whether or not it is usable.
pub struct Settings {
    pub enabled: bool,
    pub port: Option<i64>,
    pub has_password: bool,
    pub bind: IpAddr,
}

impl Settings {
    pub fn endpoint(&self, password: String) -> Option<Endpoint> {
        let port = u16::try_from(self.port?).ok()?;
        if !self.enabled || port == 0 || password.is_empty() {
            return None;
        }
        Some(Endpoint {
            address: SocketAddr::new(self.bind, port),
            password,
        })
    }
}

/// Reads the RCON settings out of a server folder. Missing file means not set up.
pub async fn settings(directory: &Path) -> Result<Option<Settings>> {
    let Some(file) = Properties::load_if_present(properties::path_in(directory)).await? else {
        return Ok(None);
    };
    Ok(Some(read(&file)))
}

fn read(file: &Properties) -> Settings {
    // The server binds RCON to server-ip when that is set, and to every
    // interface otherwise; either way loopback reaches it from here.
    let bind = file
        .get("server-ip")
        .filter(|value| !value.trim().is_empty())
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));

    Settings {
        enabled: file.flag("enable-rcon").unwrap_or(false),
        port: file.number("rcon.port"),
        has_password: file
            .get("rcon.password")
            .is_some_and(|value| !value.is_empty()),
        bind,
    }
}

/// The endpoint for a server folder, or `None` when RCON is not usable.
pub async fn endpoint(directory: &Path) -> Result<Option<Endpoint>> {
    let Some(file) = Properties::load_if_present(properties::path_in(directory)).await? else {
        return Ok(None);
    };
    let password = file.get("rcon.password").unwrap_or_default();
    Ok(read(&file).endpoint(password))
}

/// Connects, authenticates, runs one command and hangs up.
///
/// A fresh connection per command costs two round trips on loopback, which is
/// cheaper than keeping sockets alive across server restarts.
pub async fn run(endpoint: &Endpoint, command: &str) -> Result<String> {
    let mut session = Session::connect(endpoint).await?;
    session.exec(command).await
}

/// Connects and authenticates and nothing else, to prove the settings work.
pub async fn check(endpoint: &Endpoint) -> Result<()> {
    Session::connect(endpoint).await.map(|_| ())
}

struct Session {
    stream: TcpStream,
    next_id: i32,
}

struct Packet {
    id: i32,
    kind: i32,
    body: String,
}

impl Session {
    async fn connect(endpoint: &Endpoint) -> Result<Self> {
        let stream = timeout(CONNECT_TIMEOUT, TcpStream::connect(endpoint.address))
            .await
            .context("RCON did not answer in time")?
            .with_context(|| format!("connecting to RCON on {}", endpoint.address))?;
        stream.set_nodelay(true).ok();

        let mut session = Self { stream, next_id: 0 };
        let id = session.send(AUTH, &endpoint.password).await?;
        loop {
            let packet = timeout(CONNECT_TIMEOUT, session.read())
                .await
                .context("RCON stopped responding during sign-in")??;
            // Some servers send an empty value packet before the auth result.
            if packet.kind == RESPONSE {
                continue;
            }
            if packet.id == -1 || packet.id != id {
                bail!("RCON refused the password");
            }
            return Ok(session);
        }
    }

    async fn exec(&mut self, command: &str) -> Result<String> {
        let id = self.send(EXEC, command).await?;
        let first = timeout(COMMAND_TIMEOUT, self.read())
            .await
            .context("the server did not answer the command")??;
        if first.id != id {
            bail!("RCON answered a different request");
        }

        let mut out = first.body;
        // Long output arrives as several packets carrying the same id.
        while let Ok(Ok(packet)) = timeout(CONTINUATION_TIMEOUT, self.read()).await {
            if packet.id != id {
                break;
            }
            out.push_str(&packet.body);
        }
        Ok(out)
    }

    async fn send(&mut self, kind: i32, body: &str) -> Result<i32> {
        self.next_id = self.next_id.wrapping_add(1).max(1);
        let id = self.next_id;
        let body = body.as_bytes();
        let length = 10 + body.len() as i32;

        let mut packet = Vec::with_capacity(4 + length as usize);
        packet.extend_from_slice(&length.to_le_bytes());
        packet.extend_from_slice(&id.to_le_bytes());
        packet.extend_from_slice(&kind.to_le_bytes());
        packet.extend_from_slice(body);
        packet.extend_from_slice(&[0, 0]);

        self.stream.write_all(&packet).await?;
        self.stream.flush().await?;
        Ok(id)
    }

    async fn read(&mut self) -> Result<Packet> {
        let mut header = [0u8; 4];
        self.stream.read_exact(&mut header).await?;
        let length = i32::from_le_bytes(header);
        if !(10..=MAX_PACKET).contains(&length) {
            bail!("RCON sent a {length} byte packet");
        }

        let mut rest = vec![0u8; length as usize];
        self.stream.read_exact(&mut rest).await?;
        let id = i32::from_le_bytes(rest[0..4].try_into().expect("four bytes"));
        let kind = i32::from_le_bytes(rest[4..8].try_into().expect("four bytes"));
        // The body is followed by two terminating nulls.
        let body = String::from_utf8_lossy(&rest[8..rest.len() - 2]).into_owned();
        Ok(Packet { id, kind, body })
    }
}

/// A password for a listener the panel opens on the user's behalf. Avoids the
/// characters that are easy to misread when someone copies it out of the file.
pub fn generate_password() -> String {
    const ALPHABET: &[u8] = b"abcdefghijkmnopqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    (0..32)
        .map(|_| ALPHABET[rand::random_range(0..ALPHABET.len())] as char)
        .collect()
}

/// Picks a free port for a new listener, starting from the Minecraft default.
pub async fn free_port(avoid: &[i64]) -> Option<u16> {
    for port in 25575..25675u16 {
        if avoid.contains(&(port as i64)) {
            continue;
        }
        if tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, port))
            .await
            .is_ok()
        {
            return Some(port);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    async fn read_packet(stream: &mut TcpStream) -> Option<(i32, i32, String)> {
        let mut header = [0u8; 4];
        stream.read_exact(&mut header).await.ok()?;
        let length = i32::from_le_bytes(header) as usize;
        let mut rest = vec![0u8; length];
        stream.read_exact(&mut rest).await.ok()?;
        let id = i32::from_le_bytes(rest[0..4].try_into().unwrap());
        let kind = i32::from_le_bytes(rest[4..8].try_into().unwrap());
        let body = String::from_utf8_lossy(&rest[8..rest.len() - 2]).into_owned();
        Some((id, kind, body))
    }

    async fn write_packet(stream: &mut TcpStream, id: i32, kind: i32, body: &str) {
        let body = body.as_bytes();
        let length = 10 + body.len() as i32;
        let mut out = Vec::with_capacity(4 + length as usize);
        out.extend_from_slice(&length.to_le_bytes());
        out.extend_from_slice(&id.to_le_bytes());
        out.extend_from_slice(&kind.to_le_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(&[0, 0]);
        stream.write_all(&out).await.unwrap();
    }

    /// A stand-in that speaks the same wire format the game servers do.
    async fn fake_server(
        password: &'static str,
        reply: &'static [&'static str],
        empty_packet_first: bool,
    ) -> SocketAddr {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            while let Some((id, kind, body)) = read_packet(&mut stream).await {
                match kind {
                    AUTH => {
                        if empty_packet_first {
                            write_packet(&mut stream, id, RESPONSE, "").await;
                        }
                        let id = if body == password { id } else { -1 };
                        write_packet(&mut stream, id, 2, "").await;
                    }
                    EXEC => {
                        for chunk in reply {
                            write_packet(&mut stream, id, RESPONSE, chunk).await;
                        }
                    }
                    _ => {}
                }
            }
        });
        address
    }

    #[tokio::test]
    async fn runs_a_command_and_joins_split_output() {
        let address = fake_server(
            "secret",
            &["There are 0 of a max of ", "20 players online:"],
            false,
        )
        .await;
        let endpoint = Endpoint {
            address,
            password: "secret".into(),
        };
        assert_eq!(
            run(&endpoint, "list").await.unwrap(),
            "There are 0 of a max of 20 players online:"
        );
    }

    #[tokio::test]
    async fn tolerates_an_empty_packet_before_the_auth_result() {
        let address = fake_server("secret", &["ok"], true).await;
        let endpoint = Endpoint {
            address,
            password: "secret".into(),
        };
        assert_eq!(run(&endpoint, "list").await.unwrap(), "ok");
    }

    #[tokio::test]
    async fn refuses_a_bad_password() {
        let address = fake_server("secret", &[], false).await;
        let endpoint = Endpoint {
            address,
            password: "wrong".into(),
        };
        let error = check(&endpoint).await.unwrap_err().to_string();
        assert!(error.contains("refused"), "unexpected error: {error}");
    }

    async fn scratch_folder(name: &str, contents: &str) -> std::path::PathBuf {
        let directory = std::env::temp_dir().join(format!("crustation-rcon-{name}"));
        tokio::fs::create_dir_all(&directory).await.unwrap();
        tokio::fs::write(properties::path_in(&directory), contents)
            .await
            .unwrap();
        directory
    }

    #[tokio::test]
    async fn reads_an_endpoint_out_of_server_properties() {
        let directory = scratch_folder(
            "configured",
            "enable-rcon=true
rcon.port=25575
rcon.password=secret
server-ip=
",
        )
        .await;
        let endpoint = endpoint(&directory).await.unwrap().expect("configured");
        assert_eq!(endpoint.address.to_string(), "127.0.0.1:25575");
        assert_eq!(endpoint.password, "secret");
        tokio::fs::remove_dir_all(&directory).await.ok();
    }

    #[tokio::test]
    async fn treats_a_blank_password_as_not_set_up() {
        let directory = scratch_folder(
            "blank",
            "enable-rcon=true
rcon.port=25575
rcon.password=
",
        )
        .await;
        assert!(endpoint(&directory).await.unwrap().is_none());
        let settings = settings(&directory).await.unwrap().unwrap();
        assert!(settings.enabled && !settings.has_password);
        tokio::fs::remove_dir_all(&directory).await.ok();
    }

    #[tokio::test]
    async fn missing_properties_means_nothing_to_connect_to() {
        let directory = std::env::temp_dir().join("crustation-rcon-absent");
        tokio::fs::remove_dir_all(&directory).await.ok();
        tokio::fs::create_dir_all(&directory).await.unwrap();
        assert!(settings(&directory).await.unwrap().is_none());
        assert!(endpoint(&directory).await.unwrap().is_none());
        tokio::fs::remove_dir_all(&directory).await.ok();
    }
}
