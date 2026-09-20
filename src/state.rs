use std::sync::Arc;

use anyhow::Result;

use crate::config::Config;
use crate::db::Db;
use crate::events::Events;
use crate::supervisor::Supervisor;

pub struct Inner {
    pub config: Config,
    pub db: Db,
    pub events: Events,
    pub supervisor: Supervisor,
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

async fn load_or_create_secret(db: &Db) -> Result<Vec<u8>> {
    use base64::Engine as _;
    if let Some(value) = crate::db::setting(db, "session_secret").await? {
        if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(&value) {
            if bytes.len() >= 32 {
                return Ok(bytes);
            }
        }
    }
    let mut bytes = vec![0u8; 48];
    rand::fill(&mut bytes[..]);
    let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
    crate::db::set_setting(db, "session_secret", &encoded).await?;
    Ok(bytes)
}
