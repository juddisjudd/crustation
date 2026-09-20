use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Where everything lives. In Docker these are the three mounted volumes.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Paths {
    pub config: PathBuf,
    pub servers: PathBuf,
    pub backups: PathBuf,
}

impl Default for Paths {
    fn default() -> Self {
        Self {
            config: PathBuf::from("config"),
            servers: PathBuf::from("servers"),
            backups: PathBuf::from("backups"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct HttpConfig {
    pub address: IpAddr,
    pub port: u16,
    /// Public origin used for links and cookie hardening, for example https://panel.example.com.
    pub public_url: Option<String>,
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            address: IpAddr::V4(Ipv4Addr::UNSPECIFIED),
            port: 8080,
            public_url: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PanelConfig {
    /// How long a session stays valid without use.
    pub session_days: i64,
    /// Failed sign-ins from one address before it has to wait.
    pub max_login_attempts: u32,
    pub login_cooldown_seconds: u64,
    /// Console lines kept in memory per server.
    pub console_backlog: usize,
    pub stats_interval_seconds: u64,
    pub stats_retention_days: i64,
}

impl Default for PanelConfig {
    fn default() -> Self {
        Self {
            session_days: 14,
            max_login_attempts: 5,
            login_cooldown_seconds: 300,
            console_backlog: 500,
            stats_interval_seconds: 10,
            stats_retention_days: 30,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub http: HttpConfig,
    pub paths: Paths,
    pub panel: PanelConfig,
}

impl Config {
    /// Reads config/crustation.toml, writing the defaults on first run, then applies
    /// CRUSTATION_* overrides so a container can be configured without editing files.
    pub fn load(config_dir: &Path) -> Result<Self> {
        let file = config_dir.join("crustation.toml");
        let mut config: Config = if file.exists() {
            let text = std::fs::read_to_string(&file)
                .with_context(|| format!("reading {}", file.display()))?;
            toml::from_str(&text).with_context(|| format!("parsing {}", file.display()))?
        } else {
            Config {
                paths: Paths {
                    config: config_dir.to_path_buf(),
                    ..Paths::default()
                },
                ..Config::default()
            }
        };
        config.paths.config = config_dir.to_path_buf();
        config.apply_env()?;

        if !file.exists() {
            std::fs::write(&file, toml::to_string_pretty(&config)?)
                .with_context(|| format!("writing {}", file.display()))?;
        }
        Ok(config)
    }

    fn apply_env(&mut self) -> Result<()> {
        if let Ok(value) = std::env::var("CRUSTATION_PORT") {
            self.http.port = value
                .parse()
                .context("CRUSTATION_PORT must be a port number")?;
        }
        if let Ok(value) = std::env::var("CRUSTATION_ADDRESS") {
            self.http.address = value.parse().context("CRUSTATION_ADDRESS must be an IP")?;
        }
        if let Ok(value) = std::env::var("CRUSTATION_PUBLIC_URL") {
            self.http.public_url = Some(value);
        }
        if let Ok(value) = std::env::var("CRUSTATION_SERVERS_DIR") {
            self.paths.servers = PathBuf::from(value);
        }
        if let Ok(value) = std::env::var("CRUSTATION_BACKUPS_DIR") {
            self.paths.backups = PathBuf::from(value);
        }
        Ok(())
    }

    pub fn bind(&self) -> SocketAddr {
        SocketAddr::new(self.http.address, self.http.port)
    }

    pub fn database_path(&self) -> PathBuf {
        self.paths.config.join("crustation.db")
    }

    pub fn server_dir(&self, id: &uuid::Uuid) -> PathBuf {
        self.paths.servers.join(id.to_string())
    }

    #[allow(dead_code, reason = "used by the backup manager")]
    pub fn backup_dir(&self, id: &uuid::Uuid) -> PathBuf {
        self.paths.backups.join(id.to_string())
    }

    /// Cookies may only be marked Secure when the panel is actually reached over HTTPS.
    pub fn secure_cookies(&self) -> bool {
        self.http
            .public_url
            .as_deref()
            .map(|url| url.starts_with("https://"))
            .unwrap_or(false)
    }
}
