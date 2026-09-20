use std::sync::Arc;

use anyhow::Result;

use crate::config::Config;
use crate::db::Db;
use crate::events::Events;
use crate::ping::Statuses;
use crate::providers::Catalogue;
use crate::supervisor::Supervisor;

pub struct Inner {
    pub config: Config,
    pub db: Db,
    pub events: Events,
    pub supervisor: Supervisor,
    pub http: reqwest::Client,
    pub catalogue: Catalogue,
    /// What each server last said about itself over the wire.
    pub statuses: Statuses,
    /// Bedrock servers running the add-on, and what they last reported.
    pub bridges: crate::bridge::Bridges,
    /// Signing key for session cookies, generated once and kept in the database.
    pub session_secret: Vec<u8>,
    pub started_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Clone)]
pub struct AppState(Arc<Inner>);

impl AppState {
    pub async fn new(config: Config, db: Db) -> Result<Self> {
        let session_secret = load_or_create_secret(&db).await?;
        let events = Events::new();
        let supervisor = Supervisor::new(config.clone(), db.clone(), events.clone());
        Ok(Self(Arc::new(Inner {
            config,
            db,
            events,
            supervisor,
            http: http_client()?,
            catalogue: Catalogue::default(),
            statuses: Statuses::default(),
            bridges: crate::bridge::Bridges::default(),
            session_secret,
            started_at: chrono::Utc::now(),
        })))
    }
}

impl std::ops::Deref for AppState {
    type Target = Inner;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Mojang's CDN turns away anything that does not look like a browser, so the
/// Bedrock download only arrives under a browser user agent.
fn http_client() -> Result<reqwest::Client> {
    const AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";
    Ok(reqwest::Client::builder()
        .user_agent(AGENT)
        .connect_timeout(std::time::Duration::from_secs(15))
        .build()?)
}

async fn load_or_create_secret(db: &Db) -> Result<Vec<u8>> {
    use base64::Engine as _;
    if let Some(value) = crate::db::setting(db, "session_secret").await?
        && let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(&value)
        && bytes.len() >= 32
    {
        return Ok(bytes);
    }
    let mut bytes = vec![0u8; 48];
    rand::fill(&mut bytes[..]);
    let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
    crate::db::set_setting(db, "session_secret", &encoded).await?;
    Ok(bytes)
}
