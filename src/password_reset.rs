//! Getting back into the panel from a shell when nobody can sign in.

use std::path::Path;

use anyhow::{Result, bail};
use chrono::Utc;

use crate::config::Config;
use crate::db::{self, Db};

/// What a reset did, for the person at the shell.
#[derive(Debug)]
pub struct Reset {
    pub username: String,
    pub password: String,
    pub was_disabled: bool,
}

/// Runs `crustation reset-password <username>` against the database in `config_dir`.
pub async fn run(config_dir: &Path, username: Option<String>) -> Result<()> {
    let Some(username) = username else {
        bail!("usage: crustation reset-password <username>");
    };
    let path = Config::database_in(config_dir);
    // Opening a missing file would create an empty database and hide that the folder is wrong.
    if !path.exists() {
        bail!(
            "there is no database at {}. Set CRUSTATION_CONFIG_DIR to the panel's config folder.",
            path.display()
        );
    }

    let db = db::open(&path).await?;
    let reset = reset(&db, &username).await;
    db.close().await;
    let reset = reset?;

    println!("New password for {}: {}", reset.username, reset.password);
    if reset.was_disabled {
        println!("The account was disabled. It is enabled again.");
    }
    println!("Sign in with it, then change it under your account.");
    Ok(())
}

/// Gives the user a new password, ends their sessions and lifts the sign-in cooldown.
pub async fn reset(db: &Db, username: &str) -> Result<Reset> {
    let username = username.trim();
    let found: Option<(String, String, bool)> =
        sqlx::query_as("SELECT id, username, enabled FROM users WHERE username = ?")
            .bind(username)
            .fetch_optional(db)
            .await?;
    let Some((id, username, enabled)) = found else {
        let names: Vec<(String,)> = sqlx::query_as("SELECT username FROM users ORDER BY username")
            .fetch_all(db)
            .await?;
        if names.is_empty() {
            bail!("there are no users yet. The panel creates the first one when it starts.");
        }
        let names: Vec<String> = names.into_iter().map(|(name,)| name).collect();
        bail!(
            "nobody is named '{username}'. The users are: {}",
            names.join(", ")
        );
    };

    let password = crate::generate_password();
    let hash = crate::auth::hash_password(&password)?;
    let now = Utc::now().to_rfc3339();

    let mut tx = db.begin().await?;
    sqlx::query(
        "UPDATE users SET password_hash = ?, enabled = 1, sessions_valid_from = ?, updated_at = ?
         WHERE id = ?",
    )
    .bind(&hash)
    .bind(&now)
    .bind(&now)
    .bind(&id)
    .execute(&mut *tx)
    .await?;
    // Every sign-in shares one cooldown, so it has to go for this user to get straight back in.
    sqlx::query("DELETE FROM login_attempts")
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO audit_log (at, user_id, username, action) VALUES (?, ?, ?, ?)")
        .bind(&now)
        .bind(&id)
        .bind(&username)
        .bind("reset their password from the command line")
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;

    Ok(Reset {
        username,
        password,
        was_disabled: !enabled,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn database(name: &str) -> Db {
        let root = std::env::temp_dir().join(format!("crustation-reset-{name}"));
        let _ = std::fs::remove_dir_all(&root);
        db::open(&Config::database_in(&root)).await.unwrap()
    }

    async fn add_user(db: &Db, username: &str, password: &str, enabled: bool) {
        sqlx::query(
            "INSERT INTO users (id, username, password_hash, enabled, created_at, updated_at,
                                sessions_valid_from)
             VALUES (?, ?, ?, ?, '2026-01-01T00:00:00+00:00', '2026-01-01T00:00:00+00:00',
                     '2026-01-01T00:00:00+00:00')",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(username)
        .bind(crate::auth::hash_password(password).unwrap())
        .bind(enabled)
        .execute(db)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn a_reset_lets_a_shut_out_user_straight_back_in() {
        let db = database("back-in").await;
        add_user(&db, "judd", "forgotten-password", false).await;
        sqlx::query(
            "INSERT INTO login_attempts (address, at, username) VALUES ('local', ?, 'judd')",
        )
        .bind(Utc::now().to_rfc3339())
        .execute(&db)
        .await
        .unwrap();

        let reset = reset(&db, " Judd ").await.unwrap();
        assert_eq!(reset.username, "judd");
        assert!(reset.was_disabled);

        let (hash, enabled, valid_from): (String, bool, String) = sqlx::query_as(
            "SELECT password_hash, enabled, sessions_valid_from FROM users WHERE username = 'judd'",
        )
        .fetch_one(&db)
        .await
        .unwrap();
        assert!(crate::auth::verify_password(&reset.password, &hash));
        assert!(!crate::auth::verify_password("forgotten-password", &hash));
        assert!(enabled);
        assert_ne!(valid_from, "2026-01-01T00:00:00+00:00");

        let (attempts,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM login_attempts")
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(attempts, 0);
        let (entries,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM audit_log WHERE username = 'judd'")
                .fetch_one(&db)
                .await
                .unwrap();
        assert_eq!(entries, 1);
        db.close().await;
    }

    #[tokio::test]
    async fn an_unknown_name_says_who_there_is() {
        let db = database("unknown").await;
        add_user(&db, "judd", "some-password", true).await;
        add_user(&db, "alex", "other-password", true).await;

        let error = reset(&db, "admin").await.unwrap_err().to_string();
        assert!(error.contains("'admin'"), "{error}");
        assert!(error.contains("alex, judd"), "{error}");
        db.close().await;
    }
}
