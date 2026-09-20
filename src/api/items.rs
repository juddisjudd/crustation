//! Item pictures, fetched once from the gallery and then kept.
//!
//! The panel fetches rather than the browser. A panel on a home network can
//! reach the internet when the machine looking at it might not, the gallery
//! sees one request per item instead of one per person, and nothing Mojang
//! drew has to ship inside the panel's own image.

use axum::extract::{Path, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use axum::{Router, routing::get};

use crate::auth::Identity;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

/// The first eight bytes of every png. What comes back is written to disk, so
/// it is worth knowing it is a picture before believing the content type.
const PNG: &[u8] = b"\x89PNG\r\n\x1a\n";

/// An item picture is a few hundred bytes. Anything approaching this is not
/// one, and is refused rather than cached.
const MOST: usize = 512 * 1024;

/// Whether a warm is already under way. One panel, one process, one run.
static WARMING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Mounted at `/items`, beside the per-server list rather than inside it: a
/// picture of a diamond is the same picture whichever server asked.
pub fn routes() -> Router<AppState> {
    Router::new().route("/{id}/icon", get(icon))
}

/// Fills the cache from the gallery's archives, once, in the background.
///
/// One at a time works, at about a third of a second each: a picker opening
/// cold asks for two hundred and takes the best part of a minute to fill in.
/// The same set is a dozen archives, laid out as `<version>/<id>.png`, which
/// is already where the cache looks, so one pass leaves every picture local
/// and every later panel instant. A panel with no way out to the internet
/// stays empty and draws the list without pictures.
pub fn warm_in_background(state: &AppState) {
    use std::sync::atomic::Ordering;
    if WARMING.swap(true, Ordering::SeqCst) {
        return;
    }
    let state = state.clone();
    tokio::spawn(async move {
        if let Err(error) = warm(&state).await {
            tracing::info!(%error, "could not fill the item picture cache");
        }
        WARMING.store(false, Ordering::SeqCst);
    });
}

async fn warm(state: &AppState) -> anyhow::Result<()> {
    let icons = state.config.paths.config.join("icons");
    for version in versions() {
        // Written only after a set is fully unpacked, so a run cut halfway
        // through is done again rather than trusted.
        let into = icons.join(version);
        if into.join(".done").exists() {
            continue;
        }

        let url = format!("{}/images/{version}.zip", crate::items::GALLERY);
        let reply = state.http.get(&url).send().await?.error_for_status()?;
        let bytes = reply.bytes().await?;

        let archive = icons.join(format!("{version}.zip.part"));
        tokio::fs::create_dir_all(&icons).await?;
        tokio::fs::write(&archive, &bytes).await?;

        let unpack = icons.clone();
        let at = archive.clone();
        // The zip crate is synchronous, and this is a few thousand small files.
        tokio::task::spawn_blocking(move || crate::files::extract_zip(&at, &unpack, "")).await??;
        tokio::fs::remove_file(&archive).await.ok();
        tokio::fs::write(into.join(".done"), b"").await.ok();
        tracing::info!(version, "unpacked a set of item pictures");
    }
    Ok(())
}

/// Every gallery version the catalogue points at. The sets are incremental, so
/// together they are the current picture of every item.
fn versions() -> Vec<&'static str> {
    let mut seen: Vec<&'static str> = crate::items::catalogue("minecraft_java")
        .iter()
        .chain(crate::items::catalogue("minecraft_bedrock"))
        .filter_map(|one| one.icon.as_deref())
        .collect();
    seen.sort_unstable();
    seen.dedup();
    seen
}

async fn icon(
    _identity: Identity,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Response> {
    // Looked up rather than trusted. Everything that reaches the filesystem or
    // the network below is the catalogue's own id, never the one asked for, so
    // there is no path to traverse and no url to forge.
    let item = crate::items::catalogue("minecraft_java")
        .iter()
        .chain(crate::items::catalogue("minecraft_bedrock"))
        .find(|one| one.id == id)
        .ok_or_else(|| ApiError::not_found("Item"))?;
    let (Some(url), Some(version)) = (crate::items::icon_url(item), item.icon.as_deref()) else {
        // A Bedrock-only item, or one from an add-on. The interface draws a
        // letter in place of a picture, so this is ordinary rather than wrong.
        return Err(ApiError::not_found("Item picture"));
    };

    let kept = state
        .config
        .paths
        .config
        .join("icons")
        .join(version)
        .join(format!("{}.png", item.id));
    if let Ok(bytes) = tokio::fs::read(&kept).await {
        return Ok(sent(bytes));
    }

    let reply = state.http.get(&url).send().await.map_err(|error| {
        ApiError::Unavailable(format!("the item gallery is not answering: {error}"))
    })?;
    if !reply.status().is_success() {
        return Err(ApiError::not_found("Item picture"));
    }
    let bytes = reply.bytes().await.map_err(|error| {
        ApiError::Unavailable(format!("the item gallery cut the answer short: {error}"))
    })?;
    if bytes.len() > MOST || !bytes.starts_with(PNG) {
        return Err(ApiError::Unavailable(
            "the item gallery answered with something that is not a picture".to_string(),
        ));
    }

    // A picture that cannot be written is still a picture worth showing, so a
    // full or read-only disk costs the cache and nothing else.
    if let Some(parent) = kept.parent() {
        let _ = tokio::fs::create_dir_all(parent).await;
    }
    if let Err(error) = tokio::fs::write(&kept, &bytes).await {
        tracing::debug!(%error, item = %item.id, "could not keep an item picture");
    }
    Ok(sent(bytes.to_vec()))
}

/// Kept for a week by the browser. The url names the gallery version, so a
/// picture that changes changes its address with it.
fn sent(bytes: Vec<u8>) -> Response {
    (
        [
            (header::CONTENT_TYPE, "image/png"),
            (header::CACHE_CONTROL, "public, max-age=604800, immutable"),
        ],
        bytes,
    )
        .into_response()
}
