use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::{Json, Router, routing::get};
use chrono::Utc;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::FromRow;
use uuid::Uuid;

use crate::auth::Identity;
use crate::error::{ApiError, ApiResult, Created, Done, Ok as OkJson};
use crate::perms::{self, Global, Server as ServerPerm};
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(list).post(create))
        .route("/{id}", get(detail).patch(update).delete(remove))
}

#[derive(FromRow)]
struct Row {
    id: String,
    name: String,
    global_permissions: String,
    created_at: String,
}

/// What each role may do, panel-wide and on each server it reaches.
async fn role_json(state: &AppState, row: Row) -> Value {
    let servers: Vec<(String, String)> =
        sqlx::query_as("SELECT server_id, permissions FROM role_servers WHERE role_id = ?")
            .bind(&row.id)
            .fetch_all(&state.db)
            .await
            .unwrap_or_default();

    let (members,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM user_roles WHERE role_id = ?")
        .bind(&row.id)
        .fetch_one(&state.db)
        .await
        .unwrap_or((0,));

    json!({
        "id": row.id,
        "name": row.name,
        "created_at": row.created_at,
        "members": members,
        "global_permissions": names(&row.global_permissions),
        "servers": servers
            .into_iter()
            .map(|(server_id, permissions)| json!({
                "server_id": server_id,
                "permissions": perms::parse_server(&permissions)
                    .iter()
                    .map(|permission| permission.as_str())
                    .collect::<Vec<_>>(),
            }))
            .collect::<Vec<_>>(),
    })
}

fn names(stored: &str) -> Vec<&'static str> {
    perms::parse_global(stored)
        .iter()
        .map(|permission| permission.as_str())
        .collect()
}

async fn list(identity: Identity, State(state): State<AppState>) -> ApiResult<impl IntoResponse> {
    identity.require_global(Global::ManageRoles)?;
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT id, name, global_permissions, created_at FROM roles ORDER BY name COLLATE NOCASE",
    )
    .fetch_all(&state.db)
    .await?;

    let mut out = Vec::new();
    for row in rows {
        out.push(role_json(&state, row).await);
    }
    Ok(OkJson(out))
}

async fn load(state: &AppState, id: Uuid) -> Result<Row, ApiError> {
    sqlx::query_as("SELECT id, name, global_permissions, created_at FROM roles WHERE id = ?")
        .bind(id.to_string())
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| ApiError::not_found("Role"))
}

async fn detail(
    identity: Identity,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<impl IntoResponse> {
    identity.require_global(Global::ManageRoles)?;
    let row = load(&state, id).await?;
    Ok(OkJson(role_json(&state, row).await))
}

#[derive(Deserialize)]
struct ServerGrant {
    server_id: String,
    permissions: Vec<String>,
}

#[derive(Deserialize)]
struct Write {
    name: Option<String>,
    global_permissions: Option<Vec<String>>,
    servers: Option<Vec<ServerGrant>>,
}

/// Refuses a permission name the panel does not know, rather than dropping it
/// quietly and leaving somebody believing they granted it.
fn checked_global(wanted: &[String], identity: &Identity) -> Result<String, ApiError> {
    let mut out = Vec::new();
    for name in wanted {
        let permission: Global = name.parse().map_err(|_| {
            ApiError::field(
                "global_permissions",
                format!("No permission called {name}."),
            )
        })?;
        // A role that grants ADMIN is a way to mint administrators sideways.
        if permission == Global::Admin && !identity.is_admin() {
            return Err(ApiError::forbidden(
                "Only an administrator can put ADMIN in a role.",
            ));
        }
        out.push(permission);
    }
    Ok(perms::join(out))
}

fn checked_server(wanted: &[String]) -> Result<String, ApiError> {
    let mut out = Vec::new();
    for name in wanted {
        let permission: ServerPerm = name
            .parse()
            .map_err(|_| ApiError::field("servers", format!("No permission called {name}.")))?;
        out.push(permission);
    }
    Ok(perms::join(out))
}

async fn create(
    identity: Identity,
    State(state): State<AppState>,
    Json(body): Json<Write>,
) -> ApiResult<impl IntoResponse> {
    identity.require_global(Global::ManageRoles)?;
    let name = body
        .name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .ok_or_else(|| ApiError::field("name", "Give the role a name."))?
        .to_string();

    let global = checked_global(body.global_permissions.as_deref().unwrap_or(&[]), &identity)?;
    let id = Uuid::new_v4();
    let now = Utc::now().to_rfc3339();

    let result = sqlx::query(
        "INSERT INTO roles (id, name, global_permissions, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(id.to_string())
    .bind(&name)
    .bind(&global)
    .bind(&now)
    .bind(&now)
    .execute(&state.db)
    .await;

    if let Err(error) = result {
        let clash = error
            .as_database_error()
            .is_some_and(|database| database.is_unique_violation());
        return Err(match clash {
            true => ApiError::field("name", "A role already has that name."),
            false => ApiError::from(error),
        });
    }

    set_servers(&state, id, body.servers.as_deref().unwrap_or(&[])).await?;
    super::audit(
        &state,
        Some(&identity.user),
        None,
        "created a role",
        Some(&name),
    )
    .await;
    Ok(Created(json!({ "id": id })))
}

async fn update(
    identity: Identity,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<Write>,
) -> ApiResult<impl IntoResponse> {
    identity.require_global(Global::ManageRoles)?;
    let row = load(&state, id).await?;

    let name = match body.name.as_deref().map(str::trim) {
        Some("") => return Err(ApiError::field("name", "Give the role a name.")),
        Some(name) => name.to_string(),
        None => row.name.clone(),
    };
    let global = match &body.global_permissions {
        Some(wanted) => checked_global(wanted, &identity)?,
        None => row.global_permissions.clone(),
    };

    sqlx::query("UPDATE roles SET name = ?, global_permissions = ?, updated_at = ? WHERE id = ?")
        .bind(&name)
        .bind(&global)
        .bind(Utc::now().to_rfc3339())
        .bind(id.to_string())
        .execute(&state.db)
        .await?;

    if let Some(servers) = &body.servers {
        set_servers(&state, id, servers).await?;
    }

    super::audit(
        &state,
        Some(&identity.user),
        None,
        "changed a role",
        Some(&name),
    )
    .await;
    let row = load(&state, id).await?;
    Ok(OkJson(role_json(&state, row).await))
}

async fn remove(
    identity: Identity,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<impl IntoResponse> {
    identity.require_global(Global::ManageRoles)?;
    let row = load(&state, id).await?;
    // The rows in user_roles and role_servers go with it, by foreign key.
    sqlx::query("DELETE FROM roles WHERE id = ?")
        .bind(id.to_string())
        .execute(&state.db)
        .await?;
    super::audit(
        &state,
        Some(&identity.user),
        None,
        "deleted a role",
        Some(&row.name),
    )
    .await;
    Ok(Done)
}

async fn set_servers(state: &AppState, role: Uuid, grants: &[ServerGrant]) -> Result<(), ApiError> {
    sqlx::query("DELETE FROM role_servers WHERE role_id = ?")
        .bind(role.to_string())
        .execute(&state.db)
        .await?;

    for grant in grants {
        let server = Uuid::parse_str(&grant.server_id)
            .map_err(|_| ApiError::field("servers", "Unknown server."))?;
        let permissions = checked_server(&grant.permissions)?;
        if permissions.is_empty() {
            continue;
        }
        sqlx::query(
            "INSERT OR REPLACE INTO role_servers (role_id, server_id, permissions)
             VALUES (?, ?, ?)",
        )
        .bind(role.to_string())
        .bind(server.to_string())
        .bind(&permissions)
        .execute(&state.db)
        .await
        .map_err(|_| ApiError::field("servers", "Unknown server."))?;
    }
    Ok(())
}
