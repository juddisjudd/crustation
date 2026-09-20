use axum::extract::Query;
use axum::response::IntoResponse;
use axum::{Router, routing::get};
use serde::Deserialize;

use crate::auth::Identity;
use crate::error::{ApiError, ApiResult, Ok as OkJson};
use crate::properties;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/", get(list))
}

#[derive(Deserialize)]
struct Which {
    kind: String,
}

/// The settings the panel can present for one kind of server. Static reference
/// data, so any signed-in caller may read it.
async fn list(_identity: Identity, Query(which): Query<Which>) -> ApiResult<impl IntoResponse> {
    match which.kind.as_str() {
        "minecraft_java" | "minecraft_bedrock" => Ok(OkJson(properties::catalogue(&which.kind))),
        _ => Err(ApiError::not_found("Server kind")),
    }
}
