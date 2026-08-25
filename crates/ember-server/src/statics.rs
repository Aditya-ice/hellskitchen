//! Serves the exported Next.js bundle.
//!
//! `next build` with `output: 'export'` writes `out/index.html`, `out/pos.html`
//! and so on, so a request for `/pos` has to be resolved to `pos.html`. Rather
//! than reimplement file lookup (and its path-traversal pitfalls), this asks
//! `ServeDir` for each candidate in turn.

use std::sync::Arc;

use axum::body::Body;
use axum::extract::State;
use axum::http::{Request, StatusCode};
use axum::response::{IntoResponse, Response};
use tower::ServiceExt;
use tower_http::services::ServeDir;

use crate::AppState;

pub async fn serve(State(state): State<Arc<AppState>>, request: Request<Body>) -> Response {
    let Some(directory) = state.config.static_dir.clone() else {
        return (
            StatusCode::NOT_FOUND,
            axum::Json(serde_json::json!({
                "error": "No static bundle is configured. Run `npm run build` and set EMBER_STATIC_DIR, or use the Next.js dev server."
            })),
        )
            .into_response();
    };

    let path = request.uri().path().trim_start_matches('/').to_string();

    // `/pos` → pos.html → pos/index.html; `/` → index.html. A miss falls back
    // to index.html so client-side routes still boot.
    let mut candidates = vec![path.clone()];
    if path.is_empty() {
        candidates.push("index.html".into());
    } else if !path.contains('.') {
        candidates.push(format!("{path}.html"));
        candidates.push(format!("{path}/index.html"));
    }
    candidates.push("index.html".into());

    for candidate in candidates {
        let probe = Request::builder()
            .uri(format!("/{candidate}"))
            .body(Body::empty())
            .expect("a well-formed probe request");

        // ServeDir's error type is Infallible — a miss comes back as a 404.
        let Ok(response) = ServeDir::new(&directory).oneshot(probe).await;
        if response.status() == StatusCode::OK {
            let (parts, body) = response.into_parts();
            return Response::from_parts(parts, Body::new(body));
        }
    }

    (StatusCode::NOT_FOUND, "Not found").into_response()
}
