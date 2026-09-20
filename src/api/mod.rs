pub mod auth;
pub mod files;
pub mod macros;
pub mod panel;
pub mod players;
pub mod properties;
pub mod providers;
pub mod roles;
pub mod servers;
pub mod users;
pub mod ws;

use axum::Router;
use axum::routing::get;
use chrono::Utc;
use uuid::Uuid;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .nest("/auth", auth::routes())
        .nest("/servers", servers::routes())
        .nest("/panel", panel::routes())
        .nest("/providers", providers::routes())
        .nest("/properties", properties::routes())
        .nest("/users", users::routes())
        .nest("/roles", roles::routes())
        .route("/health", get(health))
}

async fn health() -> &'static str {
    "ok"
}

/// Records something worth remembering. Failures here must never fail a request.
pub async fn audit(
    state: &AppState,
    user: Option<&crate::auth::User>,
    server_id: Option<Uuid>,
    action: &str,
    detail: Option<&str>,
) {
    let result = sqlx::query(
        "INSERT INTO audit_log (at, user_id, username, server_id, action, detail, address)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(Utc::now().to_rfc3339())
    .bind(user.map(|user| user.id.clone()))
    .bind(user.map(|user| user.username.clone()))
    .bind(server_id.map(|id| id.to_string()))
    .bind(action)
    .bind(detail)
    .bind(Option::<String>::None)
    .execute(&state.db)
    .await;

    if let Err(error) = result {
        tracing::warn!(%error, "could not write an audit entry");
    }
}
