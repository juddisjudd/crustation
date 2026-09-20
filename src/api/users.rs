use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::{
    Json, Router,
    routing::{get, post},
};
use chrono::Utc;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::FromRow;
use uuid::Uuid;

use crate::auth::{self, Identity};
use crate::error::{ApiError, ApiResult, Created, Done, Ok as OkJson};
use crate::perms::Global;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(list).post(create))
        .route("/{id}", get(detail).patch(update).delete(remove))
        .route("/{id}/api-keys", get(keys).post(mint_key))
        .route(
            "/{id}/api-keys/{key_id}",
            post(revoke_key).delete(revoke_key),
        )
}

/// The shortest password the panel will accept. Long beats clever.
const MIN_PASSWORD: usize = 10;

#[derive(FromRow)]
struct Row {
    id: String,
    username: String,
    email: Option<String>,
    language: String,
    is_admin: bool,
    enabled: bool,
    created_at: String,
    last_login_at: Option<String>,
}

async fn user_json(state: &AppState, row: Row) -> Value {
    let roles: Vec<(String, String)> = sqlx::query_as(
        "SELECT r.id, r.name FROM roles r
         JOIN user_roles ur ON ur.role_id = r.id
         WHERE ur.user_id = ? ORDER BY r.name COLLATE NOCASE",
    )
    .bind(&row.id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    json!({
        "id": row.id,
        "username": row.username,
        "email": row.email,
        "language": row.language,
        "is_admin": row.is_admin,
        "enabled": row.enabled,
        "created_at": row.created_at,
        "last_login_at": row.last_login_at,
        "roles": roles.into_iter().map(|(id, name)| json!({ "id": id, "name": name })).collect::<Vec<_>>(),
    })
}

async fn list(identity: Identity, State(state): State<AppState>) -> ApiResult<impl IntoResponse> {
    identity.require_global(Global::ManageUsers)?;
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT id, username, email, language, is_admin, enabled, created_at, last_login_at
             FROM users ORDER BY username COLLATE NOCASE",
    )
    .fetch_all(&state.db)
    .await?;

    let mut out = Vec::new();
    for row in rows {
        out.push(user_json(&state, row).await);
    }
    Ok(OkJson(out))
}

async fn load(state: &AppState, id: Uuid) -> Result<Row, ApiError> {
    sqlx::query_as(
        "SELECT id, username, email, language, is_admin, enabled, created_at, last_login_at
         FROM users WHERE id = ?",
    )
    .bind(id.to_string())
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| ApiError::not_found("User"))
}

async fn detail(
    identity: Identity,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<impl IntoResponse> {
    // Anyone may read their own record; reading anybody else needs the permission.
    if identity.id() != id {
        identity.require_global(Global::ManageUsers)?;
    }
    let row = load(&state, id).await?;
    Ok(OkJson(user_json(&state, row).await))
}

#[derive(Deserialize)]
struct Create {
    username: String,
    password: String,
    email: Option<String>,
    language: Option<String>,
    #[serde(default)]
    is_admin: bool,
    #[serde(default = "yes")]
    enabled: bool,
    #[serde(default)]
    roles: Vec<String>,
}

fn yes() -> bool {
    true
}

fn check_username(username: &str) -> Result<String, ApiError> {
    let trimmed = username.trim();
    if trimmed.len() < 2 {
        return Err(ApiError::field(
            "username",
            "Give them a name of two letters or more.",
        ));
    }
    if trimmed.len() > 64 {
        return Err(ApiError::field("username", "That name is too long."));
    }
    if trimmed
        .chars()
        .any(|ch| ch.is_whitespace() || ch.is_control())
    {
        return Err(ApiError::field(
            "username",
            "A name cannot hold spaces or control characters.",
        ));
    }
    Ok(trimmed.to_string())
}

fn check_password(password: &str) -> Result<(), ApiError> {
    if password.chars().count() < MIN_PASSWORD {
        return Err(ApiError::field(
            "password",
            format!("Use at least {MIN_PASSWORD} characters."),
        ));
    }
    Ok(())
}

async fn create(
    identity: Identity,
    State(state): State<AppState>,
    Json(body): Json<Create>,
) -> ApiResult<impl IntoResponse> {
    identity.require_global(Global::ManageUsers)?;
    // Only an administrator can mint another one.
    if body.is_admin && !identity.is_admin() {
        return Err(ApiError::forbidden(
            "Only an administrator can make another administrator.",
        ));
    }

    let username = check_username(&body.username)?;
    check_password(&body.password)?;

    let id = Uuid::new_v4();
    let now = Utc::now().to_rfc3339();
    let result = sqlx::query(
        "INSERT INTO users (id, username, password_hash, email, language, is_admin, enabled,
                            created_at, updated_at, sessions_valid_from)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(id.to_string())
    .bind(&username)
    .bind(auth::hash_password(&body.password).map_err(ApiError::Internal)?)
    .bind(&body.email)
    .bind(body.language.as_deref().unwrap_or("en"))
    .bind(body.is_admin)
    .bind(body.enabled)
    .bind(&now)
    .bind(&now)
    .bind(&now)
    .execute(&state.db)
    .await;

    if let Err(error) = result {
        return Err(taken(error, "username", "Somebody already has that name."));
    }

    set_roles(&state, id, &body.roles).await?;
    super::audit(
        &state,
        Some(&identity.user),
        None,
        "created a user",
        Some(&username),
    )
    .await;
    Ok(Created(json!({ "id": id })))
}

#[derive(Deserialize)]
struct Update {
    username: Option<String>,
    password: Option<String>,
    email: Option<String>,
    language: Option<String>,
    is_admin: Option<bool>,
    enabled: Option<bool>,
    roles: Option<Vec<String>>,
}

async fn update(
    identity: Identity,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<Update>,
) -> ApiResult<impl IntoResponse> {
    identity.require_global(Global::ManageUsers)?;
    let row = load(&state, id).await?;

    if (body.is_admin.is_some() || body.roles.is_some()) && !identity.is_admin() {
        return Err(ApiError::forbidden(
            "Only an administrator can change what somebody may do.",
        ));
    }
    // Locking yourself out is the one mistake that cannot be undone from here.
    if identity.id() == id {
        if body.is_admin == Some(false) && row.is_admin {
            return Err(ApiError::conflict(
                "You cannot take administrator away from yourself.",
            ));
        }
        if body.enabled == Some(false) {
            return Err(ApiError::conflict("You cannot disable yourself."));
        }
    }

    let losing_admin = (body.is_admin == Some(false) && row.is_admin)
        || (body.enabled == Some(false) && row.enabled && row.is_admin);
    if losing_admin && admins_left(&state, &row.id).await? == 0 {
        return Err(ApiError::conflict(
            "This is the last administrator. Make somebody else one first.",
        ));
    }

    let username = match &body.username {
        Some(name) => check_username(name)?,
        None => row.username.clone(),
    };
    if let Some(password) = &body.password {
        check_password(password)?;
    }

    let now = Utc::now().to_rfc3339();
    let result = sqlx::query(
        "UPDATE users SET username = ?, email = ?, language = ?, is_admin = ?, enabled = ?,
            updated_at = ? WHERE id = ?",
    )
    .bind(&username)
    .bind(body.email.clone().or(row.email))
    .bind(body.language.unwrap_or(row.language))
    .bind(body.is_admin.unwrap_or(row.is_admin))
    .bind(body.enabled.unwrap_or(row.enabled))
    .bind(&now)
    .bind(id.to_string())
    .execute(&state.db)
    .await;

    if let Err(error) = result {
        return Err(taken(error, "username", "Somebody already has that name."));
    }

    // A new password, or being shut out, ends every session they had open.
    if body.password.is_some() || body.enabled == Some(false) {
        if let Some(password) = &body.password {
            sqlx::query("UPDATE users SET password_hash = ? WHERE id = ?")
                .bind(auth::hash_password(password).map_err(ApiError::Internal)?)
                .bind(id.to_string())
                .execute(&state.db)
                .await?;
        }
        sqlx::query("UPDATE users SET sessions_valid_from = ? WHERE id = ?")
            .bind(&now)
            .bind(id.to_string())
            .execute(&state.db)
            .await?;
    }

    if let Some(roles) = &body.roles {
        set_roles(&state, id, roles).await?;
    }

    super::audit(
        &state,
        Some(&identity.user),
        None,
        "changed a user",
        Some(&username),
    )
    .await;
    let row = load(&state, id).await?;
    Ok(OkJson(user_json(&state, row).await))
}

async fn remove(
    identity: Identity,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<impl IntoResponse> {
    identity.require_global(Global::ManageUsers)?;
    if identity.id() == id {
        return Err(ApiError::conflict("You cannot delete yourself."));
    }
    let row = load(&state, id).await?;
    if row.is_admin && row.enabled && admins_left(&state, &row.id).await? == 0 {
        return Err(ApiError::conflict(
            "This is the last administrator. Make somebody else one first.",
        ));
    }

    sqlx::query("DELETE FROM users WHERE id = ?")
        .bind(id.to_string())
        .execute(&state.db)
        .await?;
    super::audit(
        &state,
        Some(&identity.user),
        None,
        "deleted a user",
        Some(&row.username),
    )
    .await;
    Ok(Done)
}

/// How many enabled administrators there would be without this one.
async fn admins_left(state: &AppState, without: &str) -> Result<i64, ApiError> {
    let (count,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM users WHERE is_admin = 1 AND enabled = 1 AND id != ?")
            .bind(without)
            .fetch_one(&state.db)
            .await?;
    Ok(count)
}

async fn set_roles(state: &AppState, user: Uuid, roles: &[String]) -> Result<(), ApiError> {
    sqlx::query("DELETE FROM user_roles WHERE user_id = ?")
        .bind(user.to_string())
        .execute(&state.db)
        .await?;
    for role in roles {
        let role = Uuid::parse_str(role).map_err(|_| ApiError::field("roles", "Unknown role."))?;
        sqlx::query("INSERT OR IGNORE INTO user_roles (user_id, role_id) VALUES (?, ?)")
            .bind(user.to_string())
            .bind(role.to_string())
            .execute(&state.db)
            .await
            .map_err(|_| ApiError::field("roles", "Unknown role."))?;
    }
    Ok(())
}

/// SQLite reports a clash on a unique column; say which one in plain words.
fn taken(error: sqlx::Error, field: &str, message: &str) -> ApiError {
    let clash = error
        .as_database_error()
        .is_some_and(|database| database.is_unique_violation());
    match clash {
        true => ApiError::field(field, message),
        false => ApiError::from(error),
    }
}

// API keys

#[derive(FromRow)]
struct KeyRow {
    id: String,
    name: String,
    global_permissions: String,
    server_permissions: String,
    created_at: String,
    last_used_at: Option<String>,
}

/// A caller may always manage their own keys; anybody else's needs the permission.
fn may_touch_keys(identity: &Identity, owner: Uuid) -> Result<(), ApiError> {
    match identity.id() == owner {
        true => Ok(()),
        false => identity.require_global(Global::ManageUsers),
    }
}

async fn keys(
    identity: Identity,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<impl IntoResponse> {
    may_touch_keys(&identity, id)?;
    let rows: Vec<KeyRow> = sqlx::query_as(
        "SELECT id, name, global_permissions, server_permissions, created_at, last_used_at
         FROM api_keys WHERE user_id = ? ORDER BY created_at DESC",
    )
    .bind(id.to_string())
    .fetch_all(&state.db)
    .await?;

    Ok(OkJson(
        rows.into_iter()
            .map(|row| {
                json!({
                    "id": row.id,
                    "name": row.name,
                    "global_permissions": row.global_permissions,
                    "server_permissions": row.server_permissions,
                    "created_at": row.created_at,
                    "last_used_at": row.last_used_at,
                })
            })
            .collect::<Vec<_>>(),
    ))
}

#[derive(Deserialize)]
struct MintKey {
    name: String,
    /// Empty means the key may do whatever its owner may do.
    #[serde(default)]
    global_permissions: String,
    #[serde(default)]
    server_permissions: String,
}

async fn mint_key(
    identity: Identity,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<MintKey>,
) -> ApiResult<impl IntoResponse> {
    may_touch_keys(&identity, id)?;
    load(&state, id).await?;

    let name = body.name.trim();
    if name.is_empty() {
        return Err(ApiError::field("name", "Give the key a name."));
    }

    let token = auth::generate_token();
    let key = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO api_keys (id, user_id, name, token_hash, global_permissions,
                               server_permissions, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(key.to_string())
    .bind(id.to_string())
    .bind(name)
    .bind(auth::sha256_hex(&token))
    .bind(&body.global_permissions)
    .bind(&body.server_permissions)
    .bind(Utc::now().to_rfc3339())
    .execute(&state.db)
    .await?;

    super::audit(
        &state,
        Some(&identity.user),
        None,
        "created an API key",
        Some(name),
    )
    .await;
    // The only time the token is ever readable.
    Ok(Created(json!({ "id": key, "name": name, "token": token })))
}

async fn revoke_key(
    identity: Identity,
    State(state): State<AppState>,
    Path((id, key_id)): Path<(Uuid, Uuid)>,
) -> ApiResult<impl IntoResponse> {
    may_touch_keys(&identity, id)?;
    let done = sqlx::query("DELETE FROM api_keys WHERE id = ? AND user_id = ?")
        .bind(key_id.to_string())
        .bind(id.to_string())
        .execute(&state.db)
        .await?;
    if done.rows_affected() == 0 {
        return Err(ApiError::not_found("API key"));
    }
    super::audit(
        &state,
        Some(&identity.user),
        None,
        "revoked an API key",
        None,
    )
    .await;
    Ok(Done)
}
