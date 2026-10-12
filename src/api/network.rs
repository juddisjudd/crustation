use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::{Router, routing::get};
use serde_json::json;
use uuid::Uuid;

use crate::auth::Identity;
use crate::error::{ApiError, ApiResult, Ok as OkJson};
use crate::network::{self, Outside, Report};
use crate::state::AppState;
use crate::supervisor::State as ServerState;

/// Merged into the servers router, where the id in the path comes from.
pub fn routes() -> Router<AppState> {
    Router::new().route("/{id}/network", get(overview))
}

async fn overview(
    identity: Identity,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<impl IntoResponse> {
    let granted = identity
        .server_permissions(&state.db, id)
        .await
        .map_err(ApiError::Internal)?;
    if granted.is_empty() {
        return Err(ApiError::not_found("Server"));
    }
    let row = super::servers::load(&state, id).await?;
    let running = state.supervisor.state(id).await == ServerState::Running;
    let public = network::public_address(&state.http).await;

    let ports = network::ports(&row.kind, row.port, &row.command);
    let mut ports: Vec<Report> = match running {
        true => network::check(&ports, &row.host, public).await,
        false => ports.iter().map(Report::unchecked).collect(),
    };
    // Facepunch tries Rust+ from its own servers, so its failure outranks anything asked from here.
    if running && state.supervisor.rust_plus_failure(id).await.is_some() {
        for port in ports.iter_mut().filter(|port| port.name == "rust_plus") {
            port.outside = Some(Outside::Closed);
        }
    }

    Ok(OkJson(json!({
        "local": network::local_address(),
        "public": public,
        "running": running,
        "ports": ports,
    })))
}
