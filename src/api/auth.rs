use axum::extract::State;
use axum::response::IntoResponse;
use axum::{
    Json, Router,
    routing::{get, post},
};
use axum_extra::extract::CookieJar;
use axum_extra::extract::cookie::{Cookie, SameSite};
use chrono::{Duration, Utc};
use serde::Deserialize;
use serde_json::json;

use crate::auth::{self, Identity, SESSION_COOKIE};
use crate::error::{ApiError, ApiResult, Done, Ok as OkJson};
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/login", post(login))
        .route("/logout", post(logout))
        .route("/session", get(session))
        .route("/password", post(change_password))
}

#[derive(Deserialize)]
struct Login {
    username: String,
    password: String,
}

async fn login(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(body): Json<Login>,
) -> ApiResult<impl IntoResponse> {
    let address = "local".to_string();
    if let Some(retry_after) = cooldown_remaining(&state, &address).await? {
        return Err(ApiError::RateLimited { retry_after });
    }

    let user: Option<auth::User> = sqlx::query_as("SELECT * FROM users WHERE username = ?")
        .bind(body.username.trim())
        .fetch_optional(&state.db)
        .await?;

    let Some(user) = user.filter(|user| auth::verify_password(&body.password, &user.password_hash))
    else {
        record_failure(&state, &address, &body.username).await?;
        return Err(ApiError::Unauthenticated);
    };

    if !user.enabled {
        return Err(ApiError::forbidden("This account is disabled."));
    }

    let id = uuid::Uuid::parse_str(&user.id).map_err(|e| ApiError::Internal(e.into()))?;
    let token = auth::issue_session(&state.session_secret, id, state.config.panel.session_days)
        .map_err(ApiError::Internal)?;

    sqlx::query("UPDATE users SET last_login_at = ?, last_login_ip = ? WHERE id = ?")
        .bind(Utc::now().to_rfc3339())
        .bind(&address)
        .bind(&user.id)
        .execute(&state.db)
        .await?;
    sqlx::query("DELETE FROM login_attempts WHERE address = ?")
        .bind(&address)
        .execute(&state.db)
        .await?;

    crate::api::audit(&state, Some(&user), None, "signed in", None).await;

    let identity = auth::load_identity(&state.db, id)
        .await
        .map_err(ApiError::Internal)?
        .ok_or(ApiError::Unauthenticated)?;

    Ok((
        jar.add(session_cookie(&state, token)),
        OkJson(json!({ "user": user_json(&identity) })),
    ))
}

async fn logout(State(state): State<AppState>, jar: CookieJar) -> impl IntoResponse {
    let mut cookie = Cookie::new(SESSION_COOKIE, "");
    cookie.set_path("/");
    cookie.set_http_only(true);
    cookie.set_same_site(SameSite::Lax);
    cookie.set_secure(state.config.secure_cookies());
    cookie.set_max_age(time::Duration::seconds(0));
    (jar.remove(cookie), Done)
}

async fn session(
    identity: Identity,
    State(state): State<AppState>,
) -> ApiResult<impl IntoResponse> {
    let docker =
        std::env::var("CRUSTATION_DOCKER").is_ok() || std::path::Path::new("/.dockerenv").exists();
    Ok(OkJson(json!({
        "user": user_json(&identity),
        "permissions": {
            "global": crate::perms::Global::ALL
                .into_iter()
                .filter(|permission| identity.has_global(*permission))
                .map(|permission| permission.as_str())
                .collect::<Vec<_>>(),
        },
        "panel": {
            "version": env!("CARGO_PKG_VERSION"),
            "docker": docker,
            "timezone": iana_time_zone::get_timezone().unwrap_or_else(|_| "UTC".into()),
            "starting": false,
            "started_at": state.started_at,
        }
    })))
}

#[derive(Deserialize)]
struct ChangePassword {
    current_password: String,
    new_password: String,
}

async fn change_password(
    identity: Identity,
    State(state): State<AppState>,
    Json(body): Json<ChangePassword>,
) -> ApiResult<impl IntoResponse> {
    if !auth::verify_password(&body.current_password, &identity.user.password_hash) {
        return Err(ApiError::field(
            "current_password",
            "That password is wrong.",
        ));
    }
    if body.new_password.chars().count() < 10 {
        return Err(ApiError::field(
            "new_password",
            "Use at least 10 characters.",
        ));
    }
    let hash = auth::hash_password(&body.new_password).map_err(ApiError::Internal)?;
    // Signing out other sessions is the point of changing a password.
    sqlx::query(
        "UPDATE users SET password_hash = ?, sessions_valid_from = ?, updated_at = ? WHERE id = ?",
    )
    .bind(&hash)
    .bind(Utc::now().to_rfc3339())
    .bind(Utc::now().to_rfc3339())
    .bind(&identity.user.id)
    .execute(&state.db)
    .await?;
    crate::api::audit(
        &state,
        Some(&identity.user),
        None,
        "changed their password",
        None,
    )
    .await;
    Ok(Done)
}

fn session_cookie(state: &AppState, token: String) -> Cookie<'static> {
    let mut cookie = Cookie::new(SESSION_COOKIE, token);
    cookie.set_path("/");
    cookie.set_http_only(true);
    cookie.set_same_site(SameSite::Lax);
    cookie.set_secure(state.config.secure_cookies());
    cookie.set_max_age(time::Duration::days(state.config.panel.session_days));
    cookie
}

pub fn user_json(identity: &Identity) -> serde_json::Value {
    json!({
        "id": identity.user.id,
        "username": identity.user.username,
        "email": identity.user.email,
        "language": identity.user.language,
        "is_admin": identity.is_admin(),
        "created_at": identity.user.created_at,
        "last_login_at": identity.user.last_login_at,
    })
}

async fn cooldown_remaining(state: &AppState, address: &str) -> Result<Option<u64>, ApiError> {
    let window = Utc::now() - Duration::seconds(state.config.panel.login_cooldown_seconds as i64);
    let (count,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM login_attempts WHERE address = ? AND at > ?")
            .bind(address)
            .bind(window.to_rfc3339())
            .fetch_one(&state.db)
            .await?;

    if count >= state.config.panel.max_login_attempts as i64 {
        let (last,): (String,) =
            sqlx::query_as("SELECT MAX(at) FROM login_attempts WHERE address = ?")
                .bind(address)
                .fetch_one(&state.db)
                .await?;
        let last = chrono::DateTime::parse_from_rfc3339(&last)
            .map(|value| value.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now());
        let ready = last + Duration::seconds(state.config.panel.login_cooldown_seconds as i64);
        let remaining = (ready - Utc::now()).num_seconds().max(1) as u64;
        return Ok(Some(remaining));
    }
    Ok(None)
}

async fn record_failure(state: &AppState, address: &str, username: &str) -> Result<(), ApiError> {
    sqlx::query("INSERT INTO login_attempts (address, at, username) VALUES (?, ?, ?)")
        .bind(address)
        .bind(Utc::now().to_rfc3339())
        .bind(username)
        .execute(&state.db)
        .await?;
    Ok(())
}
