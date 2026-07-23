//! Loopback API contract. Seam contract: S1 and S2.

use crate::{ProjectId, RunId, ScienceError, ScienceStore, csv};
use axum::{
    Router,
    body::Body,
    extract::{Path as AxumPath, Query, State},
    http::{HeaderMap, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::get,
};
use serde::Deserialize;
use std::{
    fs,
    future::Future,
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::net::TcpListener;
use uuid::Uuid;

#[derive(Debug)]
pub struct TokenFile {
    path: PathBuf,
    token: String,
}

impl TokenFile {
    pub fn create(path: impl Into<PathBuf>) -> crate::Result<Self> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        if let Ok(metadata) = fs::symlink_metadata(&path) {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(ScienceError::Invalid(
                    "stale token path must be a regular file".into(),
                ));
            }
            fs::remove_file(&path)?;
        }
        let token = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
        #[cfg(unix)]
        {
            use std::io::Write;
            use std::os::unix::fs::OpenOptionsExt;
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&path)?;
            file.write_all(token.as_bytes())?;
            file.sync_all()?;
        }
        #[cfg(not(unix))]
        fs::write(&path, token.as_bytes())?;
        Ok(Self { path, token })
    }
    pub fn token(&self) -> &str {
        &self.token
    }
}

impl Drop for TokenFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[derive(Debug, Clone)]
pub struct ApiState {
    pub store: ScienceStore,
    pub token: String,
    pub owner_id: String,
}

pub fn router(state: ApiState) -> Router {
    Router::new()
        .route("/projects/{project}/runs/{run}", get(run))
        .route("/projects/{project}/runs/{run}/events", get(events))
        .route("/projects/{project}/runs/{run}/replay", get(events))
        .route("/projects/{project}/runs/{run}/result", get(result))
        .route(
            "/projects/{project}/runs/{run}/artifacts/{*artifact}",
            get(artifact),
        )
        .with_state(Arc::new(state))
}

/// Serve the authenticated API on a caller-owned loopback listener. Holding
/// `token_file` for the entire future makes normal shutdown remove the token;
/// the next startup removes a stale regular token left by an unclean exit.
pub async fn serve_loopback(
    listener: TcpListener,
    state: ApiState,
    token_file: TokenFile,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> crate::Result<()> {
    if !listener.local_addr()?.ip().is_loopback() {
        return Err(ScienceError::Invalid(
            "science API listener must be loopback".into(),
        ));
    }
    if state.token != token_file.token() {
        return Err(ScienceError::Invalid(
            "API state token does not match token guard".into(),
        ));
    }
    let _token_lifetime = token_file;
    axum::serve(listener, router(state))
        .with_graceful_shutdown(shutdown)
        .await?;
    Ok(())
}

#[derive(Deserialize)]
struct Replay {
    after: Option<u64>,
    limit: Option<usize>,
    token: Option<String>,
}

fn authorize(
    state: &ApiState,
    headers: &HeaderMap,
    uri: &Uri,
    query_token: Option<&str>,
) -> std::result::Result<(), Response> {
    if query_token.is_some()
        || uri
            .query()
            .is_some_and(|query| query.split('&').any(|pair| pair.starts_with("token=")))
    {
        return Err((StatusCode::BAD_REQUEST, "query token forbidden").into_response());
    }
    let host = headers
        .get("host")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    let host_name = host
        .parse::<http::uri::Authority>()
        .ok()
        .map(|authority| authority.host().to_owned())
        .unwrap_or_default();
    if !matches!(host_name.as_str(), "127.0.0.1" | "localhost" | "[::1]") {
        return Err((StatusCode::FORBIDDEN, "host forbidden").into_response());
    }
    if let Some(origin) = headers.get("origin").and_then(|value| value.to_str().ok())
        && !origin.parse::<Uri>().ok().is_some_and(|uri| {
            uri.scheme_str() == Some("http")
                && uri.authority().is_some_and(|authority| {
                    matches!(authority.host(), "127.0.0.1" | "localhost" | "[::1]")
                })
        })
    {
        return Err((StatusCode::FORBIDDEN, "origin forbidden").into_response());
    }
    let expected = format!("Bearer {}", state.token);
    if headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        != Some(expected.as_str())
    {
        return Err((StatusCode::UNAUTHORIZED, "unauthorized").into_response());
    }
    Ok(())
}

fn owned(
    state: &ApiState,
    project: &str,
    run: &str,
) -> std::result::Result<crate::RunRecord, Response> {
    let record = state.store.load_run(&RunId::new(run)).map_err(map_error)?;
    if record.context.project_id != ProjectId::new(project)
        || record.context.owner_id != state.owner_id
    {
        return Err((StatusCode::FORBIDDEN, "run forbidden").into_response());
    }
    Ok(record)
}

async fn run(
    State(state): State<Arc<ApiState>>,
    AxumPath((project, run_id)): AxumPath<(String, String)>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    if let Err(response) = authorize(&state, &headers, &uri, None) {
        return response;
    }
    match owned(&state, &project, &run_id) {
        Ok(run) => axum::Json(run).into_response(),
        Err(response) => response,
    }
}

async fn events(
    State(state): State<Arc<ApiState>>,
    AxumPath((project, run_id)): AxumPath<(String, String)>,
    Query(query): Query<Replay>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    if let Err(response) = authorize(&state, &headers, &uri, query.token.as_deref()) {
        return response;
    }
    if let Err(response) = owned(&state, &project, &run_id) {
        return response;
    }
    match state.store.events_after(
        &RunId::new(run_id),
        query.after.unwrap_or(0),
        query.limit.unwrap_or(100),
    ) {
        Ok(events) => axum::Json(events).into_response(),
        Err(error) => map_error(error),
    }
}

async fn result(
    State(state): State<Arc<ApiState>>,
    AxumPath((project, run_id)): AxumPath<(String, String)>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    if let Err(response) = authorize(&state, &headers, &uri, None) {
        return response;
    }
    let run = match owned(&state, &project, &run_id) {
        Ok(run) => run,
        Err(response) => return response,
    };
    let evidence = match state.store.evidence(&run.context.run_id) {
        Ok(items) => items,
        Err(error) => return map_error(error),
    };
    let conclusion = evidence
        .first()
        .map_or_else(String::new, |item| item.claim.clone());
    match csv::aggregate(&state.store, run, conclusion) {
        Ok(result) => axum::Json(result).into_response(),
        Err(error) => map_error(error),
    }
}

#[derive(Deserialize)]
struct ArtifactQuery {
    preview: Option<bool>,
}

async fn artifact(
    State(state): State<Arc<ApiState>>,
    AxumPath((project, run_id, artifact)): AxumPath<(String, String, String)>,
    Query(query): Query<ArtifactQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    if let Err(response) = authorize(&state, &headers, &uri, None) {
        return response;
    }
    if let Err(response) = owned(&state, &project, &run_id) {
        return response;
    }
    let path = Path::new(&artifact);
    if query.preview == Some(true) {
        // Same traversal guard as raw artifact reads; the preview record is
        // only served when it is registered to this run and path.
        if let Err(error) = crate::validate_relative(path) {
            return map_error(error);
        }
        let previews = match state.store.previews(&RunId::new(run_id)) {
            Ok(previews) => previews,
            Err(error) => return map_error(error),
        };
        return match previews.into_iter().find(|record| record.relative_path == path) {
            Some(record) => axum::Json(record).into_response(),
            None => (StatusCode::NOT_FOUND, "no preview registered for artifact")
                .into_response(),
        };
    }
    match state.store.artifact_bytes(
        &ProjectId::new(project),
        &RunId::new(run_id),
        &state.owner_id,
        path,
    ) {
        Ok(bytes) => Response::new(Body::from(bytes)),
        Err(error) => map_error(error),
    }
}

fn map_error(error: ScienceError) -> Response {
    let status = match error {
        ScienceError::Ownership => StatusCode::FORBIDDEN,
        ScienceError::Io(ref io) if io.kind() == std::io::ErrorKind::NotFound => {
            StatusCode::NOT_FOUND
        }
        ScienceError::Invalid(_) => StatusCode::BAD_REQUEST,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (status, error.to_string()).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Request;
    use tower::ServiceExt;

    const CSV: &[u8] = b"sample_id,condition,value\na,A,2\nb,A,4\n";

    fn request(uri: &str, token: Option<&str>, host: &str, origin: Option<&str>) -> Request<Body> {
        let mut builder = Request::builder().uri(uri).header("host", host);
        if let Some(token) = token {
            builder = builder.header("authorization", format!("Bearer {token}"));
        }
        if let Some(origin) = origin {
            builder = builder.header("origin", origin);
        }
        builder.body(Body::empty()).unwrap()
    }

    #[tokio::test]
    async fn all_read_surfaces_require_auth_and_owner() {
        let temp = tempfile::tempdir().unwrap();
        let store = ScienceStore::new(temp.path());
        let result = csv::execute_approved_fixture(
            &store,
            csv::fixture_context(temp.path(), ProjectId::new("p"), "alice"),
            Path::new("fixture.csv"),
            CSV,
        )
        .unwrap();
        let run = &result.run.context.run_id.0;
        let app = router(ApiState {
            store,
            token: "secret".into(),
            owner_id: "alice".into(),
        });
        for suffix in [
            "",
            "/events",
            "/replay",
            "/result",
            "/artifacts/summary.csv",
        ] {
            let uri = format!("/projects/p/runs/{run}{suffix}");
            assert_eq!(
                app.clone()
                    .oneshot(request(&uri, None, "127.0.0.1:8000", None))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::UNAUTHORIZED
            );
            assert_eq!(
                app.clone()
                    .oneshot(request(&uri, Some("secret"), "127.0.0.1:8000", None))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::OK
            );
        }
        assert_eq!(
            app.clone()
                .oneshot(request(
                    &format!("/projects/q/runs/{run}"),
                    Some("secret"),
                    "127.0.0.1:8000",
                    None
                ))
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN
        );
    }

    #[tokio::test]
    async fn query_token_host_origin_and_traversal_are_rejected() {
        let temp = tempfile::tempdir().unwrap();
        let store = ScienceStore::new(temp.path());
        let result = csv::execute_approved_fixture(
            &store,
            csv::fixture_context(temp.path(), ProjectId::new("p"), "alice"),
            Path::new("fixture.csv"),
            CSV,
        )
        .unwrap();
        let base = format!("/projects/p/runs/{}", result.run.context.run_id.0);
        let app = router(ApiState {
            store,
            token: "secret".into(),
            owner_id: "alice".into(),
        });
        assert_eq!(
            app.clone()
                .oneshot(request(
                    &format!("{base}/events?token=secret"),
                    Some("secret"),
                    "127.0.0.1:1",
                    None
                ))
                .await
                .unwrap()
                .status(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            app.clone()
                .oneshot(request(&base, Some("secret"), "evil.test", None))
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            app.clone()
                .oneshot(request(
                    &base,
                    Some("secret"),
                    "127.0.0.1:1",
                    Some("https://evil.test")
                ))
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN
        );
        let traversal = format!("{base}/artifacts/%2e%2e%2fsummary.csv");
        assert_ne!(
            app.clone()
                .oneshot(request(&traversal, Some("secret"), "127.0.0.1:1", None))
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );
        let double_encoded = format!("{base}/artifacts/%252e%252e%252fsummary.csv");
        assert_ne!(
            app.clone()
                .oneshot(request(
                    &double_encoded,
                    Some("secret"),
                    "127.0.0.1:1",
                    None
                ))
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );
        assert_eq!(
            app.oneshot(request(&base, Some("secret"), "[::1]:8080", None))
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );
    }

    #[tokio::test]
    async fn artifact_preview_requires_auth_owner_and_registered_record() {
        let temp = tempfile::tempdir().unwrap();
        let store = ScienceStore::new(temp.path());
        let imported = crate::import::execute_approved_import(
            &store,
            csv::fixture_context(temp.path(), ProjectId::new("p"), "alice"),
            Path::new("fixture.csv"),
            CSV,
        )
        .unwrap();
        let analyzed = csv::execute_approved_fixture(
            &store,
            csv::fixture_context(temp.path(), ProjectId::new("p"), "alice"),
            Path::new("fixture.csv"),
            CSV,
        )
        .unwrap();
        let app = router(ApiState {
            store,
            token: "secret".into(),
            owner_id: "alice".into(),
        });
        let import_run = &imported.run.context.run_id.0;
        let preview_uri = format!("/projects/p/runs/{import_run}/artifacts/fixture.csv?preview=true");
        assert_eq!(
            app.clone()
                .oneshot(request(&preview_uri, None, "127.0.0.1:8000", None))
                .await
                .unwrap()
                .status(),
            StatusCode::UNAUTHORIZED
        );
        let response = app
            .clone()
            .oneshot(request(&preview_uri, Some("secret"), "127.0.0.1:8000", None))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 1_000_000)
            .await
            .unwrap();
        let record: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            record["artifact_sha256"].as_str(),
            Some(imported.artifacts[0].sha256.as_str())
        );
        // A raw read of the same artifact still returns bytes, not JSON.
        let raw = app
            .clone()
            .oneshot(request(
                &format!("/projects/p/runs/{import_run}/artifacts/fixture.csv"),
                Some("secret"),
                "127.0.0.1:8000",
                None,
            ))
            .await
            .unwrap();
        assert_eq!(raw.status(), StatusCode::OK);
        let raw_body = axum::body::to_bytes(raw.into_body(), 1_000_000)
            .await
            .unwrap();
        assert_eq!(raw_body.as_ref(), CSV);
        // An artifact without a registered preview record is a 404.
        let no_preview = format!(
            "/projects/p/runs/{}/artifacts/summary.csv?preview=true",
            analyzed.run.context.run_id.0
        );
        assert_eq!(
            app.clone()
                .oneshot(request(&no_preview, Some("secret"), "127.0.0.1:8000", None))
                .await
                .unwrap()
                .status(),
            StatusCode::NOT_FOUND
        );
        // Another owner's run is forbidden even when the run id is known.
        assert_eq!(
            app.clone()
                .oneshot(request(
                    &format!("/projects/q/runs/{import_run}/artifacts/fixture.csv?preview=true"),
                    Some("secret"),
                    "127.0.0.1:8000",
                    None,
                ))
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN
        );
        // Traversal is rejected before any record lookup.
        let traversal = format!("/projects/p/runs/{import_run}/artifacts/%2e%2e%2ffixture.csv?preview=true");
        assert_ne!(
            app.oneshot(request(&traversal, Some("secret"), "127.0.0.1:8000", None))
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );
    }

    #[test]
    fn token_file_is_0600_and_removed_on_drop() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("token");
        let token = TokenFile::create(&path).unwrap();
        assert_eq!(token.token().len(), 64);
        assert!(path.exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        drop(token);
        assert!(!path.exists());

        fs::write(&path, "stale-token").unwrap();
        let replacement = TokenFile::create(&path).unwrap();
        assert_ne!(fs::read_to_string(&path).unwrap(), "stale-token");
        drop(replacement);
        assert!(!path.exists());
    }

    #[tokio::test]
    async fn real_loopback_listener_holds_and_cleans_token() {
        let temp = tempfile::tempdir().unwrap();
        let store = ScienceStore::new(temp.path().join("store"));
        let result = csv::execute_approved_fixture(
            &store,
            csv::fixture_context(temp.path(), ProjectId::new("p"), "alice"),
            Path::new("fixture.csv"),
            CSV,
        )
        .unwrap();
        let token_path = temp.path().join("api.token");
        let token_file = TokenFile::create(&token_path).unwrap();
        let token = token_file.token().to_owned();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(serve_loopback(
            listener,
            ApiState {
                store,
                token: token.clone(),
                owner_id: "alice".into(),
            },
            token_file,
            async move {
                let _ = shutdown_rx.await;
            },
        ));
        let url = format!(
            "http://{address}/projects/p/runs/{}",
            result.run.context.run_id.0
        );
        let client = reqwest::Client::new();
        assert_eq!(
            client.get(&url).send().await.unwrap().status(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            client
                .get(&url)
                .bearer_auth(&token)
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );
        assert!(token_path.exists());
        let _ = shutdown_tx.send(());
        server.await.unwrap().unwrap();
        assert!(!token_path.exists());
    }
}
