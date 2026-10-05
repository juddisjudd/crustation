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

use crate::auth::Identity;
use crate::error::{ApiError, ApiResult, Created, Done, Ok as OkJson};
use crate::perms::Server as ServerPerm;
use crate::scheduler::{self, Action, Trigger};
use crate::state::AppState;

/// Merged into the servers router, where the id in the path comes from.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/{id}/schedules", get(list).post(create))
        .route(
            "/{id}/schedules/{schedule_id}",
            get(one).patch(update).delete(remove),
        )
        .route("/{id}/schedules/{schedule_id}/run", post(run_now))
}

#[derive(FromRow)]
struct Row {
    id: String,
    name: String,
    enabled: bool,
    trigger: String,
    action: String,
    one_time: bool,
    next_run_at: Option<String>,
    last_run_at: Option<String>,
    last_result: Option<String>,
    created_at: String,
    updated_at: String,
}

impl Row {
    fn json(&self) -> Value {
        json!({
            "id": self.id,
            "name": self.name,
            "enabled": self.enabled,
            "trigger": serde_json::from_str::<Value>(&self.trigger).unwrap_or_default(),
            "action": serde_json::from_str::<Value>(&self.action).unwrap_or_default(),
            "one_time": self.one_time,
            "next_run_at": self.next_run_at,
            "last_run_at": self.last_run_at,
            "last_result": self.last_result,
            "created_at": self.created_at,
            "updated_at": self.updated_at,
        })
    }
}

async fn allowed(identity: &Identity, state: &AppState, id: Uuid) -> Result<String, ApiError> {
    identity
        .require_server(&state.db, id, ServerPerm::Schedules)
        .await?;
    Ok(super::servers::load(state, id).await?.kind)
}

async fn find(state: &AppState, id: Uuid, schedule_id: &str) -> Result<Row, ApiError> {
    sqlx::query_as("SELECT * FROM schedules WHERE id = ? AND server_id = ?")
        .bind(schedule_id)
        .bind(id.to_string())
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| ApiError::not_found("Schedule"))
}

async fn list(
    identity: Identity,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<impl IntoResponse> {
    allowed(&identity, &state, id).await?;
    let rows: Vec<Row> =
        sqlx::query_as("SELECT * FROM schedules WHERE server_id = ? ORDER BY created_at")
            .bind(id.to_string())
            .fetch_all(&state.db)
            .await?;
    Ok(OkJson(json!(
        rows.iter().map(Row::json).collect::<Vec<_>>()
    )))
}

async fn one(
    identity: Identity,
    State(state): State<AppState>,
    Path((id, schedule_id)): Path<(Uuid, String)>,
) -> ApiResult<impl IntoResponse> {
    allowed(&identity, &state, id).await?;
    Ok(OkJson(find(&state, id, &schedule_id).await?.json()))
}

#[derive(Deserialize)]
struct NewSchedule {
    name: String,
    #[serde(default = "yes")]
    enabled: bool,
    trigger: Trigger,
    action: Action,
    #[serde(default)]
    one_time: bool,
}

fn yes() -> bool {
    true
}

/// Checks a schedule against the server and the schedule it may be chained to.
async fn checked(
    state: &AppState,
    id: Uuid,
    kind: &str,
    trigger: &Trigger,
    action: &Action,
    own_id: Option<&str>,
) -> Result<Option<String>, ApiError> {
    scheduler::check(trigger, action, kind)
        .map_err(|message| ApiError::field("trigger", message))?;
    if let Trigger::After { schedule_id, .. } = trigger {
        if Some(schedule_id.as_str()) == own_id {
            return Err(ApiError::field(
                "trigger",
                "A schedule cannot follow itself.",
            ));
        }
        find(state, id, schedule_id)
            .await
            .map_err(|_| ApiError::field("trigger", "That schedule is not on this server."))?;
    }
    let next = scheduler::next_run(trigger, Utc::now())
        .map_err(|error| ApiError::field("trigger", format!("{error:#}")))?;
    Ok(next.map(|at| at.to_rfc3339()))
}

fn name(value: &str) -> Result<String, ApiError> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > 80 {
        return Err(ApiError::field(
            "name",
            "Give it a name of up to 80 characters.",
        ));
    }
    Ok(value.to_string())
}

async fn create(
    identity: Identity,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<NewSchedule>,
) -> ApiResult<impl IntoResponse> {
    let kind = allowed(&identity, &state, id).await?;
    let label = name(&body.name)?;
    let next = checked(&state, id, &kind, &body.trigger, &body.action, None).await?;
    let schedule_id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO schedules (id, server_id, name, enabled, trigger, action, one_time, \
         next_run_at, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&schedule_id)
    .bind(id.to_string())
    .bind(&label)
    .bind(body.enabled)
    .bind(serde_json::to_string(&body.trigger).unwrap_or_default())
    .bind(serde_json::to_string(&body.action).unwrap_or_default())
    .bind(body.one_time)
    .bind(next.filter(|_| body.enabled))
    .bind(&now)
    .bind(&now)
    .execute(&state.db)
    .await?;
    super::audit(
        &state,
        Some(&identity.user),
        Some(id),
        "added a schedule",
        Some(&label),
    )
    .await;
    Ok(Created(find(&state, id, &schedule_id).await?.json()))
}

#[derive(Deserialize)]
struct Changes {
    name: Option<String>,
    enabled: Option<bool>,
    trigger: Option<Trigger>,
    action: Option<Action>,
    one_time: Option<bool>,
}

async fn update(
    identity: Identity,
    State(state): State<AppState>,
    Path((id, schedule_id)): Path<(Uuid, String)>,
    Json(body): Json<Changes>,
) -> ApiResult<impl IntoResponse> {
    let kind = allowed(&identity, &state, id).await?;
    let current = find(&state, id, &schedule_id).await?;
    let label = match &body.name {
        Some(value) => name(value)?,
        None => current.name.clone(),
    };
    let trigger = match body.trigger {
        Some(trigger) => trigger,
        None => serde_json::from_str(&current.trigger)
            .map_err(|error| ApiError::Internal(error.into()))?,
    };
    let action = match body.action {
        Some(action) => action,
        None => serde_json::from_str(&current.action)
            .map_err(|error| ApiError::Internal(error.into()))?,
    };
    let enabled = body.enabled.unwrap_or(current.enabled);
    let next = checked(&state, id, &kind, &trigger, &action, Some(&schedule_id)).await?;
    sqlx::query(
        "UPDATE schedules SET name = ?, enabled = ?, trigger = ?, action = ?, one_time = ?, \
         next_run_at = ?, updated_at = ? WHERE id = ?",
    )
    .bind(&label)
    .bind(enabled)
    .bind(serde_json::to_string(&trigger).unwrap_or_default())
    .bind(serde_json::to_string(&action).unwrap_or_default())
    .bind(body.one_time.unwrap_or(current.one_time))
    .bind(next.filter(|_| enabled))
    .bind(Utc::now().to_rfc3339())
    .bind(&schedule_id)
    .execute(&state.db)
    .await?;
    super::audit(
        &state,
        Some(&identity.user),
        Some(id),
        "changed a schedule",
        Some(&label),
    )
    .await;
    Ok(OkJson(find(&state, id, &schedule_id).await?.json()))
}

async fn remove(
    identity: Identity,
    State(state): State<AppState>,
    Path((id, schedule_id)): Path<(Uuid, String)>,
) -> ApiResult<impl IntoResponse> {
    allowed(&identity, &state, id).await?;
    let current = find(&state, id, &schedule_id).await?;
    sqlx::query("DELETE FROM schedules WHERE id = ?")
        .bind(&schedule_id)
        .execute(&state.db)
        .await?;
    super::audit(
        &state,
        Some(&identity.user),
        Some(id),
        "removed a schedule",
        Some(&current.name),
    )
    .await;
    Ok(Done)
}

/// Runs a schedule straight away, in the background, as its timer would have.
async fn run_now(
    identity: Identity,
    State(state): State<AppState>,
    Path((id, schedule_id)): Path<(Uuid, String)>,
) -> ApiResult<impl IntoResponse> {
    allowed(&identity, &state, id).await?;
    let current = find(&state, id, &schedule_id).await?;
    let background = state.clone();
    tokio::spawn(async move {
        if let Err(error) = scheduler::run(&background, &schedule_id).await {
            tracing::warn!(%error, "a schedule run by hand failed");
        }
    });
    super::audit(
        &state,
        Some(&identity.user),
        Some(id),
        "ran a schedule",
        Some(&current.name),
    )
    .await;
    Ok(OkJson(json!({ "started": true })))
}
