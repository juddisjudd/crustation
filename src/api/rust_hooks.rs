use std::collections::HashMap;
use std::net::SocketAddr;

use axum::Form;
use axum::Router;
use axum::extract::{ConnectInfo, Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use base64::Engine;
use serde::Deserialize;
use uuid::Uuid;

use crate::state::AppState;

/// Not nested under `/servers` and without an `Identity`: the caller is a Rust server in
/// the same container, so only the loopback is answered.
pub fn public_routes() -> Router<AppState> {
    Router::new()
        .route("/reports/{id}", post(report))
        .route("/bans/{steam_id}", get(ban))
}

fn refused() -> Response {
    StatusCode::NOT_FOUND.into_response()
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Report {
    #[serde(default)]
    subject: String,
    #[serde(default)]
    message: String,
    #[serde(default, rename = "Type")]
    kind: i64,
    target_id: Option<String>,
    target_name: Option<String>,
    image: Option<String>,
    app_info: Option<AppInfo>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AppInfo {
    user_name: Option<String>,
}

/// Rust posting something a player sent with F7, as `data`, `userid` and `key` form fields.
async fn report(
    State(state): State<AppState>,
    ConnectInfo(caller): ConnectInfo<SocketAddr>,
    Path(id): Path<Uuid>,
    Form(form): Form<HashMap<String, String>>,
) -> Response {
    if !caller.ip().is_loopback() {
        return refused();
    }
    let command: Option<String> =
        sqlx::query_scalar("SELECT command FROM servers WHERE id = ? AND kind = ?")
            .bind(id.to_string())
            .bind(crate::rust_game::KIND)
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten();
    let expected = command
        .as_deref()
        .and_then(|command| crate::rust_game::convar(command, crate::rust_moderation::REPORTS_KEY));
    if expected.is_none() || form.get("key") != expected.as_ref() {
        return refused();
    }

    let Some(report) = form
        .get("data")
        .and_then(|data| serde_json::from_str::<Report>(data).ok())
    else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let reporter = form.get("userid").cloned().unwrap_or_default();
    let image = report
        .image
        .as_deref()
        .filter(|image| !image.is_empty())
        .and_then(|image| base64::engine::general_purpose::STANDARD.decode(image).ok());
    let stored = sqlx::query(
        "INSERT INTO rust_reports (id, server_id, received_at, reporter_id, reporter_name, kind, \
         subject, message, target_id, target_name, image) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(id.to_string())
    .bind(chrono::Utc::now().to_rfc3339())
    .bind(&reporter)
    .bind(report.app_info.and_then(|info| info.user_name))
    .bind(report.kind)
    .bind(&report.subject)
    .bind(&report.message)
    .bind(report.target_id.filter(|target| !target.is_empty()))
    .bind(report.target_name.filter(|target| !target.is_empty()))
    .bind(image)
    .execute(&state.db)
    .await;
    match stored {
        Ok(_) => {
            state.events.publish(
                crate::events::Topic::Server(id),
                "report",
                serde_json::json!({ "subject": report.subject }),
            );
            StatusCode::OK.into_response()
        }
        Err(error) => {
            tracing::warn!(%error, "could not keep an F7 report");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// Rust asking about somebody joining: 404 for nobody we have banned, the ban otherwise.
async fn ban(
    State(state): State<AppState>,
    ConnectInfo(caller): ConnectInfo<SocketAddr>,
    Path(steam_id): Path<String>,
) -> Response {
    if !caller.ip().is_loopback() || !crate::rust_players::is_steam_id(&steam_id) {
        return refused();
    }
    match crate::rust_moderation::shared_ban(&state, &steam_id).await {
        Ok(Some(ban)) => axum::Json(ban).into_response(),
        Ok(None) => refused(),
        Err(error) => {
            tracing::warn!(%error, "could not look a ban up");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}
