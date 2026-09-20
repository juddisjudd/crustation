use std::path::{Path as FsPath, PathBuf};

use axum::body::Body;
use axum::extract::{DefaultBodyLimit, Path, Query, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use axum::{
    Json, Router,
    routing::{get, post},
};
use futures::StreamExt;
use serde::Deserialize;
use serde_json::json;
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

use crate::auth::Identity;
use crate::error::{ApiError, ApiResult, Done, Ok as OkJson};
use crate::files;
use crate::perms::Server as ServerPerm;
use crate::state::AppState;

/// Merged into the servers router, where the id in the path comes from.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/{id}/files",
            get(list).post(create).patch(rearrange).delete(remove),
        )
        .route(
            "/{id}/files/content",
            get(read)
                .put(save)
                .layer(DefaultBodyLimit::max(8 * 1024 * 1024)),
        )
        .route(
            "/{id}/files/upload",
            post(upload).layer(DefaultBodyLimit::disable()),
        )
        .route("/{id}/files/download", get(download))
        .route("/{id}/files/extract", post(extract))
}

/// The server's folder, and the path inside it the caller asked about.
async fn located(
    identity: &Identity,
    state: &AppState,
    id: Uuid,
    relative: &str,
) -> Result<(PathBuf, PathBuf), ApiError> {
    identity
        .require_server(&state.db, id, ServerPerm::Files)
        .await?;
    let row = super::servers::load(state, id).await?;
    let root = PathBuf::from(&row.directory);

    let target = files::resolve(&root, relative).map_err(|refused| match refused {
        files::Refused::Outside => ApiError::forbidden(refused.to_string()),
        files::Refused::Malformed(message) => ApiError::validation(message),
    })?;
    Ok((root, target))
}

#[derive(Deserialize)]
struct At {
    #[serde(default)]
    path: String,
}

async fn list(
    identity: Identity,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(at): Query<At>,
) -> ApiResult<impl IntoResponse> {
    let (_, target) = located(&identity, &state, id, &at.path).await?;
    if !target.is_dir() {
        return Err(ApiError::not_found("Folder"));
    }
    let entries = files::list(&target).await.map_err(ApiError::Internal)?;
    Ok(OkJson(
        json!({ "path": tidy(&at.path), "entries": entries }),
    ))
}

async fn read(
    identity: Identity,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(at): Query<At>,
) -> ApiResult<impl IntoResponse> {
    let (_, target) = located(&identity, &state, id, &at.path).await?;
    let metadata = tokio::fs::metadata(&target)
        .await
        .map_err(|_| ApiError::not_found("File"))?;
    if metadata.is_dir() {
        return Err(ApiError::validation("That is a folder, not a file."));
    }
    if metadata.len() > files::MAX_EDIT_BYTES {
        return Err(ApiError::validation(
            "That file is too big to edit here. Download it instead.",
        ));
    }

    let bytes = tokio::fs::read(&target).await?;
    if files::looks_binary(&bytes) {
        return Err(ApiError::validation(
            "That file is not text. Download it instead.",
        ));
    }
    let content = String::from_utf8(bytes)
        .map_err(|_| ApiError::validation("That file is not text. Download it instead."))?;

    Ok(OkJson(json!({
        "path": tidy(&at.path),
        "content": content,
        "size": metadata.len(),
        "modified": files::modified_at(&metadata),
    })))
}

#[derive(Deserialize)]
struct Save {
    path: String,
    content: String,
    /// What the caller believes the file was last changed at, so two people
    /// editing the same file do not quietly overwrite each other.
    modified: Option<String>,
}

async fn save(
    identity: Identity,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<Save>,
) -> ApiResult<impl IntoResponse> {
    let (_, target) = located(&identity, &state, id, &body.path).await?;
    if target.is_dir() {
        return Err(ApiError::validation("That is a folder, not a file."));
    }

    if let (Some(expected), Ok(metadata)) = (&body.modified, tokio::fs::metadata(&target).await)
        && files::modified_at(&metadata).as_deref() != Some(expected.as_str())
    {
        return Err(ApiError::conflict(
            "Somebody else changed this file. Reload it and try again.",
        ));
    }

    if let Some(parent) = target.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(&target, body.content.as_bytes()).await?;

    let metadata = tokio::fs::metadata(&target).await?;
    audit(&state, &identity, id, "edited a file", &body.path).await;
    Ok(OkJson(json!({
        "size": metadata.len(),
        "modified": files::modified_at(&metadata),
    })))
}

#[derive(Deserialize)]
struct Create {
    path: String,
    /// `file` or `directory`.
    kind: String,
}

async fn create(
    identity: Identity,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<Create>,
) -> ApiResult<impl IntoResponse> {
    let (_, target) = located(&identity, &state, id, &body.path).await?;
    if tokio::fs::try_exists(&target).await.unwrap_or(false) {
        return Err(ApiError::conflict("Something is already called that."));
    }

    match body.kind.as_str() {
        "directory" => tokio::fs::create_dir_all(&target).await?,
        "file" => {
            if let Some(parent) = target.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            tokio::fs::write(&target, b"").await?;
        }
        _ => return Err(ApiError::field("kind", "Choose a file or a directory.")),
    }

    audit(&state, &identity, id, "created a file", &body.path).await;
    Ok(Done)
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Rearrange {
    Rename {
        path: String,
        new_name: String,
    },
    Transfer {
        paths: Vec<String>,
        destination: String,
        /// `copy` or `move`.
        mode: String,
    },
}

async fn rearrange(
    identity: Identity,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<Rearrange>,
) -> ApiResult<impl IntoResponse> {
    match body {
        Rearrange::Rename { path, new_name } => {
            files::check_name(&new_name)
                .map_err(|error| ApiError::field("new_name", error.to_string()))?;
            let (_, target) = located(&identity, &state, id, &path).await?;
            let parent = target
                .parent()
                .ok_or_else(|| ApiError::forbidden("That path has nowhere to go."))?;
            let renamed = parent.join(new_name.trim());
            if tokio::fs::try_exists(&renamed).await.unwrap_or(false) {
                return Err(ApiError::conflict("Something is already called that."));
            }
            tokio::fs::rename(&target, &renamed).await?;
            audit(&state, &identity, id, "renamed a file", &path).await;
        }
        Rearrange::Transfer {
            paths,
            destination,
            mode,
        } => {
            if !matches!(mode.as_str(), "copy" | "move") {
                return Err(ApiError::field("mode", "Choose copy or move."));
            }
            let (_, into) = located(&identity, &state, id, &destination).await?;
            if !into.is_dir() {
                return Err(ApiError::validation("The destination is not a folder."));
            }

            for path in &paths {
                let (_, source) = located(&identity, &state, id, path).await?;
                let name = source
                    .file_name()
                    .ok_or_else(|| ApiError::validation("That path has no name."))?;
                let target = into.join(name);
                if target == source {
                    continue;
                }
                // Moving a folder into itself would run forever.
                if source.is_dir() && target.starts_with(&source) {
                    return Err(ApiError::validation("A folder cannot go inside itself."));
                }
                if tokio::fs::try_exists(&target).await.unwrap_or(false) {
                    return Err(ApiError::conflict(format!(
                        "{} is already there.",
                        name.to_string_lossy()
                    )));
                }

                match (mode.as_str(), source.is_dir()) {
                    ("move", _) => tokio::fs::rename(&source, &target).await?,
                    ("copy", true) => {
                        let (from, to) = (source.clone(), target.clone());
                        tokio::task::spawn_blocking(move || files::copy_tree(&from, &to))
                            .await
                            .map_err(|error| ApiError::Internal(error.into()))?
                            .map_err(ApiError::Internal)?;
                    }
                    ("copy", false) => {
                        tokio::fs::copy(&source, &target).await?;
                    }
                    _ => unreachable!("the mode was checked above"),
                }
            }
            audit(
                &state,
                &identity,
                id,
                &format!("{mode}d files"),
                &destination,
            )
            .await;
        }
    }
    Ok(Done)
}

#[derive(Deserialize)]
struct Remove {
    paths: Vec<String>,
}

async fn remove(
    identity: Identity,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<Remove>,
) -> ApiResult<impl IntoResponse> {
    for path in &body.paths {
        let (root, target) = located(&identity, &state, id, path).await?;
        if target == root {
            return Err(ApiError::forbidden(
                "The server folder itself cannot be deleted here.",
            ));
        }
        let result = match target.is_dir() {
            true => tokio::fs::remove_dir_all(&target).await,
            false => tokio::fs::remove_file(&target).await,
        };
        if let Err(error) = result
            && error.kind() != std::io::ErrorKind::NotFound
        {
            return Err(error.into());
        }
    }
    audit(
        &state,
        &identity,
        id,
        "deleted files",
        &body.paths.join(", "),
    )
    .await;
    Ok(Done)
}

async fn upload(
    identity: Identity,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(at): Query<At>,
    body: Body,
) -> ApiResult<impl IntoResponse> {
    let (_, target) = located(&identity, &state, id, &at.path).await?;
    if target.is_dir() {
        return Err(ApiError::validation("Name the file, not the folder."));
    }
    if let Some(parent) = target.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let mut file = tokio::fs::File::create(&target).await?;
    let mut stream = body.into_data_stream();
    let mut written: u64 = 0;
    while let Some(chunk) = stream.next().await {
        let chunk =
            chunk.map_err(|error| ApiError::validation(format!("The upload stopped: {error}")))?;
        written += chunk.len() as u64;
        file.write_all(&chunk).await?;
    }
    file.flush().await?;

    audit(&state, &identity, id, "uploaded a file", &at.path).await;
    Ok(OkJson(json!({ "size": written })))
}

async fn download(
    identity: Identity,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(at): Query<At>,
) -> Result<Response, ApiError> {
    let (_, target) = located(&identity, &state, id, &at.path).await?;
    let metadata = tokio::fs::metadata(&target)
        .await
        .map_err(|_| ApiError::not_found("File"))?;

    let (path, name, temporary) = match metadata.is_dir() {
        false => {
            let name = target
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_else(|| "download".into());
            (target, name, false)
        }
        true => {
            // The zip is built on the servers volume rather than inside the
            // server, so it never shows up in the operator's own file listing.
            // Swept on the next download, so nothing accumulates.
            let scratch = state.config.paths.servers.join(".downloads");
            tokio::fs::create_dir_all(&scratch).await?;
            sweep(&scratch).await;

            let stem = target
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_else(|| "server".into());
            let archive = scratch.join(format!("{}-{}.zip", stem, Uuid::new_v4()));
            let (from, to) = (target.clone(), archive.clone());
            tokio::task::spawn_blocking(move || files::zip_tree(&from, &to))
                .await
                .map_err(|error| ApiError::Internal(error.into()))?
                .map_err(ApiError::Internal)?;
            (archive, format!("{stem}.zip"), true)
        }
    };

    let file = tokio::fs::File::open(&path).await?;
    let stream = tokio_util::io::ReaderStream::new(file);
    let disposition = format!("attachment; filename=\"{}\"", name.replace('"', ""));

    audit(&state, &identity, id, "downloaded a file", &at.path).await;
    let mut response = (
        [
            (header::CONTENT_TYPE, "application/octet-stream".to_string()),
            (header::CONTENT_DISPOSITION, disposition),
        ],
        Body::from_stream(stream),
    )
        .into_response();
    if !temporary {
        response
            .headers_mut()
            .insert(header::CONTENT_LENGTH, metadata.len().into());
    }
    Ok(response)
}

#[derive(Deserialize)]
struct Extract {
    path: String,
}

async fn extract(
    identity: Identity,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<Extract>,
) -> ApiResult<impl IntoResponse> {
    let (_, target) = located(&identity, &state, id, &body.path).await?;
    if !target.is_file() {
        return Err(ApiError::not_found("Archive"));
    }
    let into = target
        .parent()
        .ok_or_else(|| ApiError::validation("That archive has nowhere to unpack to."))?
        .to_path_buf();

    tokio::task::spawn_blocking(move || files::extract_zip(&target, &into, ""))
        .await
        .map_err(|error| ApiError::Internal(error.into()))?
        .map_err(|error| ApiError::validation(format!("Could not unpack it: {error}")))?;

    audit(&state, &identity, id, "unpacked an archive", &body.path).await;
    Ok(Done)
}

/// Drops download zips nobody collected.
async fn sweep(directory: &FsPath) {
    let Ok(mut entries) = tokio::fs::read_dir(directory).await else {
        return;
    };
    let cutoff = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
    while let Ok(Some(entry)) = entries.next_entry().await {
        let stale = entry
            .metadata()
            .await
            .ok()
            .and_then(|data| data.modified().ok())
            .is_some_and(|modified| modified < cutoff);
        if stale {
            tokio::fs::remove_file(entry.path()).await.ok();
        }
    }
}

async fn audit(state: &AppState, identity: &Identity, id: Uuid, what: &str, detail: &str) {
    super::audit(state, Some(&identity.user), Some(id), what, Some(detail)).await;
}

/// The path as the interface should show it: no leading slash, forward slashes.
fn tidy(path: &str) -> String {
    path.replace('\\', "/")
        .trim_matches('/')
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tidies_a_path_for_display() {
        assert_eq!(tidy("/world/region/"), "world/region");
        assert_eq!(tidy("world\\region"), "world/region");
        assert_eq!(tidy(""), "");
        assert_eq!(tidy("/"), "");
        assert_eq!(tidy("./world"), "world");
    }
}
