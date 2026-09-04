//! Shared test harness.
//!
//! Every route that reads or changes the floor now requires a staff session, so
//! a test that does not sign in is exercising the 401 path whether it means to
//! or not. This provisions a manager PIN directly on the store, signs in over
//! the real login route, and hands back a client that carries the cookie.

// Each suite uses a different subset of this, and Rust compiles the module
// separately into each one, so whatever a given suite does not touch reads as
// dead code there.
#![allow(dead_code)]

use axum::body::Body;
use axum::http::{Request, StatusCode};
use chrono::Utc;
use ember_server::{AppState, Config};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt;

/// The staff member the tests act as. A manager, so role-gated routes are
/// reachable; tests that care about a lesser role sign in as someone else.
pub const STAFF_ID: &str = "manager-1";
pub const PIN: &str = "246810";

pub struct TestApp {
    pub router: axum::Router,
    /// The `Cookie` header value carrying the session.
    pub session: String,
}

/// A router with a signed-in manager.
pub async fn signed_in(config: Config) -> TestApp {
    signed_in_as(config, STAFF_ID, PIN).await
}

/// A router signed in as a particular staff member, for role-gating tests.
pub async fn signed_in_as(config: Config, staff_id: &str, pin: &str) -> TestApp {
    let state = AppState::new(config).expect("in-memory store");
    state
        .store
        .set_staff_pin(staff_id, pin, Utc::now())
        .expect("the PIN is set");
    let router = ember_server::router(state);

    let request = Request::builder()
        .method("POST")
        .uri("/api/auth/login")
        .header("host", "localhost:4000")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({ "staffId": staff_id, "pin": pin, "terminalId": "test" }).to_string(),
        ))
        .unwrap();

    let response = router
        .clone()
        .oneshot(request)
        .await
        .expect("login responds");
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "the harness must be able to sign in"
    );

    let cookie = response
        .headers()
        .get(axum::http::header::SET_COOKIE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .expect("login issues a session cookie")
        .to_string();

    TestApp {
        router,
        session: cookie,
    }
}

pub async fn signed_in_default() -> TestApp {
    signed_in(Config::default()).await
}

impl TestApp {
    /// Sends a request with the session attached.
    pub async fn send(&self, request: Request<Body>) -> (StatusCode, Value) {
        let mut request = request;
        request.headers_mut().insert(
            axum::http::header::COOKIE,
            self.session.parse().expect("a well-formed cookie"),
        );
        dispatch(&self.router, request).await
    }

    /// The raw response, for tests that assert on headers.
    pub async fn send_raw(&self, request: Request<Body>) -> axum::response::Response {
        let mut request = request;
        request.headers_mut().insert(
            axum::http::header::COOKIE,
            self.session.parse().expect("a well-formed cookie"),
        );
        self.router
            .clone()
            .oneshot(request)
            .await
            .expect("router responds")
    }

    /// Sends a request with no session, for testing the guard itself.
    pub async fn send_anonymous(&self, request: Request<Body>) -> (StatusCode, Value) {
        dispatch(&self.router, request).await
    }
}

async fn dispatch(router: &axum::Router, request: Request<Body>) -> (StatusCode, Value) {
    let response = router
        .clone()
        .oneshot(request)
        .await
        .expect("router responds");
    let status = response.status();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body collected")
        .to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

pub fn get(uri: &str) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .header("host", "localhost:4000")
        .body(Body::empty())
        .unwrap()
}

pub fn post(uri: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("host", "localhost:4000")
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}
