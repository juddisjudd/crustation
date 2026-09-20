use std::collections::BTreeSet;

use anyhow::{Context, Result, anyhow};
use argon2::password_hash::phc::PasswordHash;
use argon2::{Argon2, PasswordHasher, PasswordVerifier};
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum_extra::extract::CookieJar;
use chrono::{DateTime, Duration, Utc};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use crate::db::Db;
use crate::error::ApiError;
use crate::perms::{self, Global, Server};
use crate::state::AppState;

pub const SESSION_COOKIE: &str = "crustation_session";

pub fn hash_password(password: &str) -> Result<String> {
    Argon2::default()
        .hash_password(password.as_bytes())
        .map(|hash| hash.to_string())
        .map_err(|e| anyhow!("hashing password: {e}"))
}

pub fn verify_password(password: &str, hash: &str) -> bool {
    match PasswordHash::new(hash) {
        Ok(parsed) => Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok(),
        Err(_) => false,
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: String,
    iat: i64,
    exp: i64,
}

pub fn issue_session(secret: &[u8], user_id: Uuid, days: i64) -> Result<String> {
    let now = Utc::now();
    let claims = Claims {
        sub: user_id.to_string(),
        iat: now.timestamp(),
        exp: (now + Duration::days(days)).timestamp(),
    };
    jsonwebtoken::encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret),
    )
    .context("signing session")
}

fn read_session(secret: &[u8], token: &str) -> Option<(Uuid, DateTime<Utc>)> {
    let data = jsonwebtoken::decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret),
        &Validation::default(),
    )
    .ok()?;
    let id = Uuid::parse_str(&data.claims.sub).ok()?;
    let issued = DateTime::from_timestamp(data.claims.iat, 0)?;
    Some((id, issued))
}

#[derive(Debug, Clone, FromRow)]
pub struct User {
    pub id: String,
    pub username: String,
    pub password_hash: String,
    pub email: Option<String>,
    pub language: String,
    pub is_admin: bool,
    pub enabled: bool,
    pub created_at: String,
    pub last_login_at: Option<String>,
    #[allow(dead_code, reason = "read by the users screen")]
    pub last_login_ip: Option<String>,
    pub sessions_valid_from: String,
}

/// Who is making the request, and what they may do.
#[derive(Debug, Clone)]
pub struct Identity {
    pub user: User,
    pub global: BTreeSet<Global>,
    /// Set when the caller authenticated with an API key rather than a session.
    pub api_key_id: Option<String>,
}

impl Identity {
    #[allow(dead_code, reason = "used by ownership checks in the users API")]
    pub fn id(&self) -> Uuid {
        Uuid::parse_str(&self.user.id).unwrap_or_default()
    }

    pub fn is_admin(&self) -> bool {
        self.user.is_admin || self.global.contains(&Global::Admin)
    }

    pub fn has_global(&self, permission: Global) -> bool {
        self.is_admin() || self.global.contains(&permission)
    }

    pub fn require_global(&self, permission: Global) -> Result<(), ApiError> {
        if self.has_global(permission) {
            Ok(())
        } else {
            Err(ApiError::forbidden(format!(
                "You need the {permission} permission."
            )))
        }
    }

    /// Permissions this identity holds on one server, through its roles.
    pub async fn server_permissions(&self, db: &Db, server_id: Uuid) -> Result<BTreeSet<Server>> {
        if self.is_admin() {
            return Ok(Server::ALL.into_iter().collect());
        }
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT rs.permissions
             FROM role_servers rs
             JOIN user_roles ur ON ur.role_id = rs.role_id
             WHERE ur.user_id = ? AND rs.server_id = ?",
        )
        .bind(&self.user.id)
        .bind(server_id.to_string())
        .fetch_all(db)
        .await?;

        let mut granted = BTreeSet::new();
        for (value,) in rows {
            granted.extend(perms::parse_server(&value));
        }
        Ok(granted)
    }

    pub async fn require_server(
        &self,
        db: &Db,
        server_id: Uuid,
        permission: Server,
    ) -> Result<BTreeSet<Server>, ApiError> {
        let granted = self
            .server_permissions(db, server_id)
            .await
            .map_err(ApiError::Internal)?;
        if granted.contains(&permission) {
            Ok(granted)
        } else if granted.is_empty() {
            // Don't confirm the server exists to someone with no access to it.
            Err(ApiError::not_found("Server"))
        } else {
            Err(ApiError::forbidden(format!(
                "You need the {permission} permission on this server."
            )))
        }
    }

    /// Server ids this identity can see at all.
    pub async fn visible_servers(&self, db: &Db) -> Result<Option<Vec<String>>> {
        if self.is_admin() {
            return Ok(None); // None means "everything"
        }
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT DISTINCT rs.server_id
             FROM role_servers rs
             JOIN user_roles ur ON ur.role_id = rs.role_id
             WHERE ur.user_id = ? AND rs.permissions <> ''",
        )
        .bind(&self.user.id)
        .fetch_all(db)
        .await?;
        Ok(Some(rows.into_iter().map(|(id,)| id).collect()))
    }
}

pub async fn load_identity(db: &Db, user_id: Uuid) -> Result<Option<Identity>> {
    let user: Option<User> = sqlx::query_as("SELECT * FROM users WHERE id = ?")
        .bind(user_id.to_string())
        .fetch_optional(db)
        .await?;
    let Some(user) = user else {
        return Ok(None);
    };
    if !user.enabled {
        return Ok(None);
    }

    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT r.global_permissions
         FROM roles r JOIN user_roles ur ON ur.role_id = r.id
         WHERE ur.user_id = ?",
    )
    .bind(&user.id)
    .fetch_all(db)
    .await?;

    let mut global = BTreeSet::new();
    for (value,) in rows {
        global.extend(perms::parse_global(&value));
    }

    Ok(Some(Identity {
        user,
        global,
        api_key_id: None,
    }))
}

impl FromRequestParts<AppState> for Identity {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        if let Some(header) = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "))
        {
            return identity_from_api_key(state, header.trim()).await;
        }

        let jar = CookieJar::from_headers(&parts.headers);
        let token = jar
            .get(SESSION_COOKIE)
            .map(|cookie| cookie.value().to_string())
            .ok_or(ApiError::Unauthenticated)?;

        let (user_id, issued_at) =
            read_session(&state.session_secret, &token).ok_or(ApiError::Unauthenticated)?;

        let identity = load_identity(&state.db, user_id)
            .await
            .map_err(ApiError::Internal)?
            .ok_or(ApiError::Unauthenticated)?;

        // Password changes and "sign out everywhere" invalidate older sessions.
        if let Ok(valid_from) = DateTime::parse_from_rfc3339(&identity.user.sessions_valid_from) {
            if issued_at < valid_from.with_timezone(&Utc) {
                return Err(ApiError::Unauthenticated);
            }
        }
        Ok(identity)
    }
}

async fn identity_from_api_key(state: &AppState, token: &str) -> Result<Identity, ApiError> {
    let hash = sha256_hex(token);
    let row: Option<(String, String, String)> =
        sqlx::query_as("SELECT id, user_id, global_permissions FROM api_keys WHERE token_hash = ?")
            .bind(&hash)
            .fetch_optional(&state.db)
            .await?;

    let (key_id, user_id, key_global) = row.ok_or(ApiError::Unauthenticated)?;
    let user_id = Uuid::parse_str(&user_id).map_err(|_| ApiError::Unauthenticated)?;
    let mut identity = load_identity(&state.db, user_id)
        .await
        .map_err(ApiError::Internal)?
        .ok_or(ApiError::Unauthenticated)?;

    // A key can only narrow what its owner may do.
    if !key_global.is_empty() {
        let allowed = perms::parse_global(&key_global);
        identity
            .global
            .retain(|permission| allowed.contains(permission));
        if !allowed.contains(&Global::Admin) {
            identity.user.is_admin = false;
        }
    }
    identity.api_key_id = Some(key_id.clone());

    sqlx::query("UPDATE api_keys SET last_used_at = ? WHERE id = ?")
        .bind(Utc::now().to_rfc3339())
        .bind(&key_id)
        .execute(&state.db)
        .await?;

    Ok(identity)
}

pub fn sha256_hex(value: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
