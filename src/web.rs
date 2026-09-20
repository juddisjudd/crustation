use std::path::PathBuf;

use axum::Router;
use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;

use crate::state::AppState;

/// Where the built interface lives. Overridable so `cargo run` can point at a dev build.
fn web_dir() -> PathBuf {
    std::env::var("CRUSTATION_WEB_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("web/build"))
}

pub fn router(state: AppState) -> Router {
    let dir = web_dir();
    let index = dir.join("index.html");

    let spa = ServeDir::new(&dir)
        .precompressed_gzip()
        .fallback(ServeFile::new(index));

    Router::new()
        .nest("/api/v1", crate::api::router())
        .route("/ws", get(crate::api::ws::handler))
        .fallback_service(spa)
        .method_not_allowed_fallback(not_found)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// Anything under /api that does not exist should answer as the API, not as the SPA.
async fn not_found(uri: Uri) -> Response {
    if uri.path().starts_with("/api/") {
        return (
            StatusCode::NOT_FOUND,
            [(header::CONTENT_TYPE, "application/json")],
            r#"{"status":"error","error":"NOT_FOUND","message":"No such endpoint."}"#,
        )
            .into_response();
    }
    (StatusCode::NOT_FOUND, "Not found").into_response()
}
