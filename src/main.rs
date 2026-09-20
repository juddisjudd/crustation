mod api;
mod auth;
mod bridge;
mod config;
mod db;
mod error;
mod events;
mod files;
mod install;
mod java;
mod nbt;
mod packs;
mod perms;
mod ping;
mod properties;
mod providers;
mod rcon;
mod state;
mod stats;
mod supervisor;
mod web;

use std::path::PathBuf;

use anyhow::{Context, Result};
use chrono::Utc;
use tracing_subscriber::EnvFilter;
use uuid::Uuid;

use crate::config::Config;
use crate::state::AppState;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_env("CRUSTATION_LOG")
                .unwrap_or_else(|_| EnvFilter::new("info,sqlx=warn,tower_http=info")),
        )
        .with_target(false)
        .init();

    let config_dir = std::env::var("CRUSTATION_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("config"));
    std::fs::create_dir_all(&config_dir)
        .with_context(|| format!("creating {}", config_dir.display()))?;

    let config = Config::load(&config_dir)?;
    std::fs::create_dir_all(&config.paths.servers)?;
    std::fs::create_dir_all(&config.paths.backups)?;

    let db = db::open(&config.database_path()).await?;
    let state = AppState::new(config, db).await?;

    first_run(&state).await?;

    let app = web::router(state.clone());
    let bind = state.config.bind();
    let listener = tokio::net::TcpListener::bind(bind)
        .await
        .with_context(|| format!("binding {bind}"))?;

    tokio::spawn(stats::collect(state.clone()));
    tokio::spawn(autostart(state.clone()));

    tracing::info!("Crustation is listening on http://{bind}");

    let supervisor = state.supervisor.clone();
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            shutdown_signal().await;
            tracing::info!("stopping game servers");
            supervisor.stop_all().await;
        })
        .await?;

    Ok(())
}

/// Creates the first administrator and writes the password where the operator can find it.
async fn first_run(state: &AppState) -> Result<()> {
    let (users,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
        .fetch_one(&state.db)
        .await?;
    if users > 0 {
        return Ok(());
    }

    let username =
        std::env::var("CRUSTATION_ADMIN_USERNAME").unwrap_or_else(|_| "admin".to_string());
    let (password, generated) = match std::env::var("CRUSTATION_ADMIN_PASSWORD") {
        Ok(value) if value.chars().count() >= 10 => (value, false),
        _ => (generate_password(), true),
    };

    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO users (id, username, password_hash, language, is_admin, enabled,
                            created_at, updated_at, sessions_valid_from)
         VALUES (?, ?, ?, 'en', 1, 1, ?, ?, ?)",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&username)
    .bind(auth::hash_password(&password)?)
    .bind(&now)
    .bind(&now)
    .bind(&now)
    .execute(&state.db)
    .await?;

    if generated {
        let path = state.config.paths.config.join("first-login.txt");
        std::fs::write(
            &path,
            format!(
                "Crustation first sign-in\n\nusername: {username}\npassword: {password}\n\n\
                 Change the password after signing in; this file can then be deleted.\n"
            ),
        )?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        }
        tracing::warn!(
            "Created the administrator '{username}'. The password is in {}",
            path.display()
        );
    } else {
        tracing::info!("Created the administrator '{username}' from the environment.");
    }
    Ok(())
}

fn generate_password() -> String {
    const ALPHABET: &[u8] = b"abcdefghijkmnopqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    (0..20)
        .map(|_| ALPHABET[rand::random_range(0..ALPHABET.len())] as char)
        .collect()
}

/// Starts servers marked autostart, honouring their delay.
async fn autostart(state: AppState) {
    let rows: Vec<(String, i64)> =
        match sqlx::query_as("SELECT id, autostart_delay FROM servers WHERE autostart = 1")
            .fetch_all(&state.db)
            .await
        {
            Ok(rows) => rows,
            Err(error) => {
                tracing::warn!(%error, "could not read autostart servers");
                return;
            }
        };

    for (id, delay) in rows {
        let Ok(id) = Uuid::parse_str(&id) else {
            continue;
        };
        let state = state.clone();
        tokio::spawn(async move {
            if delay > 0 {
                tokio::time::sleep(std::time::Duration::from_secs(delay as u64)).await;
            }
            if let Err(error) = state.supervisor.start_by_id(id).await {
                tracing::warn!(%error, server = %id, "autostart failed");
            }
        });
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c().await.ok();
    };

    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut signal) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            signal.recv().await;
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }
}
