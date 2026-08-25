//! Exercises the sponsor proxies against a local stub upstream.
//!
//! No real credentials are involved: a dummy key is configured and
//! `EMBER_ELEVENLABS_BASE` / `EMBER_TAVILY_BASE` are pointed at a stub server
//! that records what it received. That covers the half of the proxy code the
//! keyless tests cannot reach — the request shape actually sent to the vendor,
//! and how failures are handled.

use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, Request, StatusCode};
use axum::response::IntoResponse;
use axum::routing::post;
use axum::{Json, Router};
use ember_server::{AppState, Config};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt;

const SESSION: &str = "ember_demo_session=123e4567-e89b-12d3-a456-426614174000";

/// What the stub upstream saw.
#[derive(Debug, Clone, Default)]
struct Recorded {
    path: String,
    authorization: Option<String>,
    xi_api_key: Option<String>,
    body: Value,
}

#[derive(Clone)]
struct StubState {
    recorded: Arc<Mutex<Vec<Recorded>>>,
    /// When true the stub fails every call, so the failure paths are covered.
    fail: bool,
}

async fn record(
    State(stub): State<StubState>,
    headers: HeaderMap,
    request: Request<Body>,
) -> axum::response::Response {
    let path = request.uri().path().to_string();
    let bytes = request
        .into_body()
        .collect()
        .await
        .map(|collected| collected.to_bytes())
        .unwrap_or_default();

    stub.recorded.lock().unwrap().push(Recorded {
        path: path.clone(),
        authorization: headers
            .get("authorization")
            .and_then(|value| value.to_str().ok())
            .map(str::to_string),
        xi_api_key: headers
            .get("xi-api-key")
            .and_then(|value| value.to_str().ok())
            .map(str::to_string),
        body: serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    });

    if stub.fail {
        return (StatusCode::INTERNAL_SERVER_ERROR, "upstream is down").into_response();
    }

    if path.starts_with("/v1/single-use-token/") {
        return Json(json!({ "token": "dummy-scribe-token" })).into_response();
    }
    Json(json!({
        "answer": "Golden beets are at their sweetest in early autumn.",
        "results": [
            {
                "title": "Golden beets",
                "url": "https://example.com/golden-beets",
                "content": "Milder and less earthy than red beets.",
                "raw_content": null
            }
        ],
        "response_time": 0.42
    }))
    .into_response()
}

/// Starts the stub and returns its base URL plus the recording buffer.
async fn stub_upstream(fail: bool) -> (String, Arc<Mutex<Vec<Recorded>>>) {
    let recorded = Arc::new(Mutex::new(Vec::new()));
    let state = StubState {
        recorded: recorded.clone(),
        fail,
    };
    let app = Router::new()
        .route("/search", post(record))
        .route("/v1/single-use-token/{token_type}", post(record))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    (format!("http://{address}"), recorded)
}

fn app_with(base: &str, fail_keys: bool) -> axum::Router {
    let config = Config {
        // Dummy credentials — the stub does not check them, but the proxy must
        // still send them in the right header.
        elevenlabs_key: (!fail_keys).then(|| "dummy-elevenlabs-key".to_string()),
        tavily_key: (!fail_keys).then(|| "dummy-tavily-key".to_string()),
        elevenlabs_base: base.to_string(),
        tavily_base: base.to_string(),
        ..Config::default()
    };
    ember_server::router(AppState::new(config).expect("in-memory store"))
}

async fn send(app: &axum::Router, request: Request<Body>) -> (StatusCode, Value) {
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

fn token_request() -> Request<Body> {
    Request::builder()
        .uri("/api/elevenlabs/token")
        .header("host", "localhost:4000")
        .header("cookie", SESSION)
        .body(Body::empty())
        .unwrap()
}

fn search_request(dish_id: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/api/tavily/search")
        .header("host", "localhost:4000")
        .header("content-type", "application/json")
        .header("cookie", SESSION)
        .body(Body::from(json!({ "dishId": dish_id }).to_string()))
        .unwrap()
}

#[tokio::test]
async fn a_scribe_token_is_fetched_and_the_key_never_reaches_the_client() {
    let (base, recorded) = stub_upstream(false).await;
    let app = app_with(&base, false);

    let (status, body) = send(&app, token_request()).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["token"], "dummy-scribe-token");
    assert_eq!(body["configured"], true);
    assert!(
        !body.to_string().contains("dummy-elevenlabs-key"),
        "the server key must never be echoed to the browser"
    );

    let calls = recorded.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].path, "/v1/single-use-token/realtime_scribe");
    assert_eq!(
        calls[0].xi_api_key.as_deref(),
        Some("dummy-elevenlabs-key"),
        "ElevenLabs authenticates with the xi-api-key header"
    );
}

#[tokio::test]
async fn dish_context_is_mapped_onto_the_ui_shape() {
    let (base, recorded) = stub_upstream(false).await;
    let app = app_with(&base, false);

    let (status, body) = send(&app, search_request("beet-salad")).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["isFallback"], false);
    assert!(body["answer"].as_str().unwrap().contains("Golden beets"));
    assert_eq!(body["sources"].as_array().unwrap().len(), 1);
    assert_eq!(body["sources"][0]["url"], "https://example.com/golden-beets");

    let calls = recorded.lock().unwrap();
    assert_eq!(calls[0].path, "/search");
    assert_eq!(
        calls[0].authorization.as_deref(),
        Some("Bearer dummy-tavily-key"),
        "Tavily authenticates with a bearer token"
    );

    // The request body must keep the vendor's snake_case field names.
    let sent = &calls[0].body;
    assert_eq!(sent["search_depth"], "basic");
    assert_eq!(sent["max_results"], 3);
    assert_eq!(sent["include_answer"], "basic");
    assert_eq!(sent["topic"], "general");

    let query = sent["query"].as_str().unwrap();
    assert!(query.contains("Golden Beet & Citrus"), "{query}");
    assert!(
        query.contains("Do not provide medical or allergy safety claims"),
        "the guardrail must survive the port: {query}"
    );
}

#[tokio::test]
async fn an_upstream_failure_does_not_take_voice_notes_down_with_it() {
    let (base, _) = stub_upstream(true).await;
    let app = app_with(&base, false);

    let (status, body) = send(&app, token_request()).await;

    assert_eq!(status, StatusCode::BAD_GATEWAY);
    assert!(body["error"]
        .as_str()
        .unwrap()
        .contains("temporarily unavailable"));
}

#[tokio::test]
async fn an_upstream_failure_degrades_dish_context_to_the_fallback() {
    let (base, _) = stub_upstream(true).await;
    let app = app_with(&base, false);

    let (status, body) = send(&app, search_request("beet-salad")).await;

    // A failed lookup is cosmetic, so the route stays 200 and says so.
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["isFallback"], true);
    assert!(body["sources"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn an_unreachable_upstream_is_handled() {
    // Nothing is listening on this port.
    let app = app_with("http://127.0.0.1:1", false);

    let (status, _) = send(&app, token_request()).await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);

    let (status, body) = send(&app, search_request("beet-salad")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["isFallback"], true);
}

#[tokio::test]
async fn the_upstream_is_never_called_without_a_session() {
    let (base, recorded) = stub_upstream(false).await;
    let app = app_with(&base, false);

    let request = Request::builder()
        .uri("/api/elevenlabs/token")
        .header("host", "localhost:4000")
        .body(Body::empty())
        .unwrap();
    let (status, _) = send(&app, request).await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(
        recorded.lock().unwrap().is_empty(),
        "an unauthenticated request must not spend sponsor quota"
    );
}

#[tokio::test]
async fn rate_limiting_stops_a_client_from_burning_sponsor_quota() {
    let (base, recorded) = stub_upstream(false).await;
    let app = app_with(&base, false);

    // The token route allows six calls per minute.
    for _ in 0..6 {
        let (status, _) = send(&app, token_request()).await;
        assert_eq!(status, StatusCode::OK);
    }

    let response = app.clone().oneshot(token_request()).await.unwrap();
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(
        response.headers().contains_key("retry-after"),
        "a 429 should tell the client when to come back"
    );

    assert_eq!(
        recorded.lock().unwrap().len(),
        6,
        "the seventh call must be stopped before it reaches the vendor"
    );
}
