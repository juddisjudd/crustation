use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::{Router, routing::get};

use crate::auth::Identity;
use crate::error::{ApiError, ApiResult, Ok as OkJson};
use crate::perms::Global;
use crate::providers;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(list))
        .route("/{provider}/versions", get(versions))
}

async fn list(identity: Identity) -> ApiResult<impl IntoResponse> {
    identity.require_global(Global::CreateServer)?;
    Ok(OkJson(providers::ALL))
}

async fn versions(
    identity: Identity,
    State(state): State<AppState>,
    Path(provider): Path<String>,
) -> ApiResult<impl IntoResponse> {
    identity.require_global(Global::CreateServer)?;
    if providers::find(&provider).is_none() {
        return Err(ApiError::not_found("Provider"));
    }

    // Upstream being down is not the panel's fault, so say so plainly.
    let versions = state
        .catalogue
        .versions(&state.http, &provider)
        .await
        .map_err(|error| {
            tracing::warn!(%error, %provider, "could not list versions");
            ApiError::Unavailable(format!(
                "Could not reach the {provider} download service. Try again shortly."
            ))
        })?;

    Ok(OkJson(versions))
}
