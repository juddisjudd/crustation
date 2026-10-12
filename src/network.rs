use std::net::{IpAddr, Ipv4Addr, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use futures::future::join_all;
use serde::Serialize;
use tokio::net::TcpStream;
use tokio::sync::Mutex;

use crate::ping;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    Tcp,
    Udp,
}

/// The question a port is asked to prove something is there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ask {
    Connect,
    Raknet,
    Source,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Port {
    pub name: &'static str,
    pub port: u16,
    pub protocol: Protocol,
    ask: Ask,
}

/// The ports players and their apps reach a server on. RCON is left out: it is the panel's own.
pub fn ports(kind: &str, port: i64, command: &str) -> Vec<Port> {
    let named = |name, port: i64, protocol, ask| {
        u16::try_from(port).ok().map(|port| Port {
            name,
            port,
            protocol,
            ask,
        })
    };
    let found = match kind {
        crate::rust_game::KIND => {
            let app = crate::rust_game::convar(command, "app.port")
                .and_then(|value| value.parse::<i64>().ok())
                .unwrap_or(0);
            // Left unset, Rust takes the higher of the game and RCON ports, plus 67.
            let app = match app {
                0 => crate::rust_game::rcon_port(port) + 67,
                other => other,
            };
            vec![
                named("game", port, Protocol::Udp, Ask::Raknet),
                named(
                    "query",
                    crate::rust_game::query_port(port),
                    Protocol::Udp,
                    Ask::Source,
                ),
                named("rust_plus", app, Protocol::Tcp, Ask::Connect),
            ]
        }
        "minecraft_bedrock" => vec![named("game", port, Protocol::Udp, Ask::Raknet)],
        _ => vec![named("game", port, Protocol::Tcp, Ask::Connect)],
    };
    found.into_iter().flatten().collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Outside {
    Open,
    Closed,
    Unanswered,
    Untested,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub name: &'static str,
    pub port: u16,
    pub protocol: Protocol,
    pub listening: Option<bool>,
    pub outside: Option<Outside>,
}

impl Report {
    pub fn unchecked(port: &Port) -> Self {
        Self {
            name: port.name,
            port: port.port,
            protocol: port.protocol,
            listening: None,
            outside: None,
        }
    }
}

const NEAR: Duration = Duration::from_secs(1);
const FAR: Duration = Duration::from_secs(2);

/// Set once anything answers through the public address: proof the router sends that back in.
static LOOPS_BACK: AtomicBool = AtomicBool::new(false);

pub async fn check(ports: &[Port], host: &str, public: Option<Ipv4Addr>) -> Vec<Report> {
    let seen = probe(ports, host, public).await;
    if seen.iter().any(|(_, far)| *far == Some(true)) {
        LOOPS_BACK.store(true, Ordering::Relaxed);
    }
    let loops_back = LOOPS_BACK.load(Ordering::Relaxed);
    ports
        .iter()
        .zip(seen)
        .map(|(port, (near, far))| {
            let listening = listening(bound(port.protocol, port.port), near, port.protocol);
            Report {
                listening,
                outside: verdict(listening, far, loops_back),
                ..Report::unchecked(port)
            }
        })
        .collect()
}

/// For each port: whether it answered nearby, then through the public address if that was asked.
async fn probe(ports: &[Port], host: &str, public: Option<Ipv4Addr>) -> Vec<(bool, Option<bool>)> {
    let host = ping::reachable_host(host);
    join_all(ports.iter().map(|port| async move {
        let near = answers(port.ask, host, port.port, NEAR).await;
        let far = match (near, public) {
            (true, Some(public)) => {
                Some(answers(port.ask, &public.to_string(), port.port, FAR).await)
            }
            _ => None,
        };
        (near, far)
    }))
    .await
}

async fn answers(ask: Ask, host: &str, port: u16, within: Duration) -> bool {
    let asked = async {
        match ask {
            Ask::Connect => TcpStream::connect((host, port)).await.is_ok(),
            Ask::Raknet => ping::raknet_pong(host, port).await.is_ok(),
            Ask::Source => ping::source(host, port).await.is_ok(),
        }
    };
    tokio::time::timeout(within, asked).await.unwrap_or(false)
}

fn listening(bound: Option<bool>, answered: bool, protocol: Protocol) -> Option<bool> {
    match (answered, bound) {
        (true, _) => Some(true),
        (false, Some(held)) => Some(held),
        // A refused TCP connection is an answer; a silent UDP port could be anything.
        (false, None) => (protocol == Protocol::Tcp).then_some(false),
    }
}

fn verdict(listening: Option<bool>, far: Option<bool>, loops_back: bool) -> Option<Outside> {
    if listening != Some(true) {
        return None;
    }
    Some(match far {
        None => Outside::Untested,
        Some(true) => Outside::Open,
        Some(false) if loops_back => Outside::Closed,
        Some(false) => Outside::Unanswered,
    })
}

/// Whether the system has a socket on the port, which only Linux says.
#[cfg(target_os = "linux")]
fn bound(protocol: Protocol, port: u16) -> Option<bool> {
    let tables = match protocol {
        Protocol::Tcp => ["/proc/net/tcp", "/proc/net/tcp6"],
        Protocol::Udp => ["/proc/net/udp", "/proc/net/udp6"],
    };
    let read: Vec<String> = tables
        .iter()
        .filter_map(|table| std::fs::read_to_string(table).ok())
        .collect();
    (!read.is_empty()).then(|| read.iter().any(|table| holds(table, protocol, port)))
}

#[cfg(not(target_os = "linux"))]
fn bound(_protocol: Protocol, _port: u16) -> Option<bool> {
    None
}

/// Reads one of `/proc/net`'s socket tables: `sl local_address rem_address st …`, ports in hex.
#[cfg(any(target_os = "linux", test))]
fn holds(table: &str, protocol: Protocol, port: u16) -> bool {
    const LISTEN: &str = "0A";
    table.lines().skip(1).any(|line| {
        let mut fields = line.split_whitespace().skip(1);
        let (Some(local), Some(_remote), Some(state)) =
            (fields.next(), fields.next(), fields.next())
        else {
            return false;
        };
        let at = local
            .rsplit(':')
            .next()
            .and_then(|hex| u16::from_str_radix(hex, 16).ok());
        at == Some(port) && (protocol == Protocol::Udp || state == LISTEN)
    })
}

const ECHOES: [&str; 2] = [
    "https://checkip.amazonaws.com",
    "https://ipv4.icanhazip.com",
];
const FRESH: Duration = Duration::from_secs(600);
const RETRY: Duration = Duration::from_secs(60);

static PUBLIC: Mutex<Option<(Instant, Option<Ipv4Addr>)>> = Mutex::const_new(None);

/// The address the internet sees this machine at, asked of an echo service every ten minutes.
pub async fn public_address(http: &reqwest::Client) -> Option<Ipv4Addr> {
    let mut known = PUBLIC.lock().await;
    if let Some((at, address)) = *known {
        let keep = if address.is_some() { FRESH } else { RETRY };
        if at.elapsed() < keep {
            return address;
        }
    }
    let mut found = None;
    for echo in ECHOES {
        found = echoed(http, echo).await;
        if found.is_some() {
            break;
        }
    }
    *known = Some((Instant::now(), found));
    found
}

async fn echoed(http: &reqwest::Client, url: &str) -> Option<Ipv4Addr> {
    let reply = http
        .get(url)
        .timeout(Duration::from_secs(4))
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?;
    public_from(&reply.text().await.ok()?)
}

fn public_from(text: &str) -> Option<Ipv4Addr> {
    let address: Ipv4Addr = text.trim().parse().ok()?;
    let routable = !(address.is_private()
        || address.is_loopback()
        || address.is_link_local()
        || address.is_unspecified());
    routable.then_some(address)
}

pub fn in_container() -> bool {
    std::env::var("CRUSTATION_DOCKER").is_ok() || std::path::Path::new("/.dockerenv").exists()
}

/// The panel's address on the local network. A container only sees the one Docker gave it.
pub fn local_address() -> Option<Ipv4Addr> {
    if in_container() {
        return None;
    }
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    // Connecting a UDP socket sends nothing; it only makes the system choose an interface.
    socket.connect((Ipv4Addr::new(1, 1, 1, 1), 53)).ok()?;
    match socket.local_addr().ok()?.ip() {
        IpAddr::V4(address) if address.is_private() => Some(address),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(ports: &[Port]) -> Vec<(&'static str, u16, Protocol)> {
        ports
            .iter()
            .map(|port| (port.name, port.port, port.protocol))
            .collect()
    }

    #[test]
    fn a_rust_server_has_its_game_query_and_rust_plus_ports() {
        let command = "./RustDedicated -batchmode +server.port 28015 +rcon.port 28016";
        assert_eq!(
            named(&ports("rust", 28015, command)),
            [
                ("game", 28015, Protocol::Udp),
                ("query", 28017, Protocol::Udp),
                ("rust_plus", 28083, Protocol::Tcp),
            ]
        );
    }

    #[test]
    fn rust_plus_follows_its_own_setting() {
        let moved = "./RustDedicated +server.port 28015 +app.port 28090";
        assert!(named(&ports("rust", 28015, moved)).contains(&("rust_plus", 28090, Protocol::Tcp)));

        let off = "./RustDedicated +server.port 28015 +app.port -1";
        assert_eq!(
            named(&ports("rust", 28015, off)),
            [
                ("game", 28015, Protocol::Udp),
                ("query", 28017, Protocol::Udp)
            ]
        );
    }

    #[test]
    fn minecraft_has_the_one_port() {
        assert_eq!(
            named(&ports("minecraft_java", 25565, "")),
            [("game", 25565, Protocol::Tcp)]
        );
        assert_eq!(
            named(&ports("minecraft_bedrock", 19132, "")),
            [("game", 19132, Protocol::Udp)]
        );
    }

    const TCP_TABLE: &str = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode
   0: 00000000:6D93 00000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 9001 1 0000000000000000 100 0 0 10 0
   1: 0100007F:6D70 0100007F:D431 01 00000000:00000000 00:00000000 00000000  1000        0 9002 1 0000000000000000 20 4 30 10 -1";

    const UDP6_TABLE: &str = "  sl  local_address                         remote_address                        st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode ref pointer drops
 2271: 00000000000000000000000000000000:6D6F 00000000000000000000000000000000:0000 07 00000000:00000000 00:00000000 00000000  1000        0 9003 2 0000000000000000 0";

    #[test]
    fn reads_which_ports_the_system_holds() {
        assert!(holds(TCP_TABLE, Protocol::Tcp, 28051));
        assert!(holds(UDP6_TABLE, Protocol::Udp, 28015));
        assert!(!holds(UDP6_TABLE, Protocol::Udp, 28017));
    }

    #[test]
    fn a_connection_on_a_port_is_not_a_listener() {
        assert!(!holds(TCP_TABLE, Protocol::Tcp, 28016));
    }

    #[test]
    fn a_port_listens_when_it_answers_or_the_system_says_so() {
        assert_eq!(listening(None, true, Protocol::Udp), Some(true));
        assert_eq!(listening(Some(true), false, Protocol::Udp), Some(true));
        assert_eq!(listening(Some(false), false, Protocol::Udp), Some(false));
        assert_eq!(listening(None, false, Protocol::Udp), None);
        assert_eq!(listening(None, false, Protocol::Tcp), Some(false));
    }

    #[test]
    fn silence_only_means_closed_once_the_router_is_known_to_loop_back() {
        assert_eq!(verdict(Some(true), Some(true), false), Some(Outside::Open));
        assert_eq!(
            verdict(Some(true), Some(false), false),
            Some(Outside::Unanswered)
        );
        assert_eq!(
            verdict(Some(true), Some(false), true),
            Some(Outside::Closed)
        );
        assert_eq!(verdict(Some(true), None, true), Some(Outside::Untested));
        assert_eq!(verdict(Some(false), None, true), None);
        assert_eq!(verdict(None, None, true), None);
    }

    #[test]
    fn takes_a_public_address_and_nothing_else() {
        assert_eq!(
            public_from("203.0.113.7\n"),
            Some(Ipv4Addr::new(203, 0, 113, 7))
        );
        assert_eq!(public_from("192.168.1.20"), None);
        assert_eq!(public_from("127.0.0.1"), None);
        assert_eq!(public_from("<html>blocked</html>"), None);
    }

    #[tokio::test]
    async fn asks_a_port_nearby_and_then_through_the_public_address() {
        let tcp = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let raknet = tokio::net::UdpSocket::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let silent = tokio::net::UdpSocket::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let port = |name, port, protocol, ask| Port {
            name,
            port,
            protocol,
            ask,
        };
        let asked = [
            port(
                "rust_plus",
                tcp.local_addr().unwrap().port(),
                Protocol::Tcp,
                Ask::Connect,
            ),
            port(
                "game",
                raknet.local_addr().unwrap().port(),
                Protocol::Udp,
                Ask::Raknet,
            ),
            port(
                "query",
                silent.local_addr().unwrap().port(),
                Protocol::Udp,
                Ask::Source,
            ),
        ];
        tokio::spawn(async move {
            let mut ping = [0u8; 64];
            while let Ok((_, from)) = raknet.recv_from(&mut ping).await {
                raknet.send_to(&[0x1c], from).await.ok();
            }
        });

        let reached = probe(&asked, "127.0.0.1", Some(Ipv4Addr::LOCALHOST)).await;
        assert_eq!(
            reached,
            [(true, Some(true)), (true, Some(true)), (false, None)]
        );

        let unknown = probe(&asked[..1], "0.0.0.0", None).await;
        assert_eq!(unknown, [(true, None)]);
    }
}
