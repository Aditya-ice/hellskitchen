//! End-to-end tests over the real router, exercised in-process.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use ember_server::{AppState, Config};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt;

const SESSION: &str = "ember_demo_session=123e4567-e89b-12d3-a456-426614174000";

fn app() -> axum::Router {
    let state = AppState::new(Config::default()).expect("in-memory store");
    ember_server::router(state)
}

async fn send(app: &axum::Router, request: Request<Body>) -> (StatusCode, Value) {
    let response = app.clone().oneshot(request).await.expect("router responds");
    let status = response.status();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body collected")
        .to_bytes();
    let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, body)
}

fn get(uri: &str) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .header("host", "localhost:4000")
        .body(Body::empty())
        .unwrap()
}

fn post(uri: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("host", "localhost:4000")
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn seat(id: &str, guest: &str, table: &str) -> Value {
    json!({
        "id": id,
        "at": "2026-08-13T10:00:00.000Z",
        "type": "seat-guest",
        "guestId": guest,
        "tableId": table,
    })
}

#[tokio::test]
async fn state_starts_from_the_seeded_service() {
    let app = app();
    let (status, body) = send(&app, get("/api/state")).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["version"], 0);
    assert_eq!(body["state"]["guests"].as_array().unwrap().len(), 4);
    assert_eq!(body["state"]["tables"].as_array().unwrap().len(), 9);
}

#[tokio::test]
async fn an_action_advances_the_revision() {
    let app = app();
    let (status, body) = send(&app, post("/api/actions", seat("a1", "guest-maya", "t2"))).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["outcome"], "changed");
    assert_eq!(body["version"], 1);

    let seated = body["state"]["tables"]
        .as_array()
        .unwrap()
        .iter()
        .find(|table| table["id"] == "t2")
        .unwrap()
        .clone();
    assert_eq!(seated["status"], "occupied");
    assert_eq!(seated["seatedGuestId"], "guest-maya");
}

#[tokio::test]
async fn a_replayed_action_id_is_reported_as_a_duplicate() {
    let app = app();
    send(&app, post("/api/actions", seat("a1", "guest-maya", "t2"))).await;
    let (status, body) = send(&app, post("/api/actions", seat("a1", "guest-maya", "t2"))).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["outcome"], "duplicate");
    assert_eq!(body["version"], 1, "a retry must not advance the revision");
}

#[tokio::test]
async fn a_guarded_action_is_reported_as_rejected() {
    let app = app();
    // Jordan is still "expected" — not checked in.
    let (status, body) = send(&app, post("/api/actions", seat("a1", "guest-jordan", "t7"))).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["outcome"], "rejected");
    assert_eq!(body["version"], 0);
}

#[tokio::test]
async fn recommendations_rank_the_best_table_first() {
    let app = app();
    let (status, body) = send(&app, get("/api/recommendations/guest-maya")).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["tables"][0]["id"], "t2");
    assert_eq!(body["tables"][0]["score"], 100.0);
    assert_eq!(body["estimateWait"], 0.0);

    // The allergen block must survive the trip through HTTP.
    let tartare = body["dishes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|dish| dish["id"] == "carrot-tartare")
        .unwrap()
        .clone();
    assert_eq!(tartare["eligible"], false);
    assert!(tartare["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|warning| warning == "Contains guest allergen: tree nuts"));
}

#[tokio::test]
async fn the_summary_reports_the_floor() {
    let app = app();
    let (status, body) = send(&app, get("/api/summary")).await;

    assert_eq!(status, StatusCode::OK);
    // Maya and Priya are waiting; Noah is seated and Jordan has not arrived.
    assert_eq!(body["waitingGuests"], 2);
    assert_eq!(body["openTables"], 6);
    // Both have a table free right now, so nobody is actually waiting on one.
    assert_eq!(body["averageWaitMinutes"], 0.0);
    assert_eq!(body["version"], 0);
}

#[tokio::test]
async fn the_summary_follows_the_floor() {
    let app = app();
    send(&app, post("/api/actions", seat("a1", "guest-maya", "t2"))).await;

    let (_, body) = send(&app, get("/api/summary")).await;
    assert_eq!(body["waitingGuests"], 1, "Maya is seated now");
    assert_eq!(body["openTables"], 5);
    assert_eq!(body["version"], 1);
}

#[tokio::test]
async fn an_unknown_guest_is_a_404() {
    let app = app();
    let (status, _) = send(&app, get("/api/recommendations/nobody")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn the_menu_is_served_from_the_rust_seed() {
    let app = app();
    let (status, body) = send(&app, get("/api/menu")).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["menuItems"].as_array().unwrap().len(), 9);
    assert_eq!(body["restaurant"]["name"], "Ember & Ash");
    assert!(
        body["ingredients"].is_null(),
        "stock moves during a service, so it belongs to state, not to reference data"
    );
}

#[tokio::test]
async fn stock_is_part_of_the_pushed_state() {
    let app = app();
    let (_, body) = send(&app, get("/api/state")).await;

    let carrots = body["state"]["ingredients"]
        .as_array()
        .expect("ingredients travel with the state")
        .iter()
        .find(|item| item["id"] == "carrot")
        .unwrap()
        .clone();
    assert_eq!(carrots["onHand"], 3.0);
}

#[tokio::test]
async fn firing_tickets_depletes_stock_and_takes_the_dish_off_the_menu() {
    let app = app();

    let step = |id: &str, kind: Value| {
        let mut action = json!({ "id": id, "at": "2026-08-13T10:00:00.000Z" });
        action
            .as_object_mut()
            .unwrap()
            .extend(kind.as_object().unwrap().clone());
        action
    };

    send(&app, post("/api/actions", seat("a1", "guest-maya", "t2"))).await;

    // Cedar Salmon needs carrots, and only three are on hand.
    for id in ["a2", "a3", "a4"] {
        send(
            &app,
            post(
                "/api/actions",
                step(
                    id,
                    json!({ "type": "add-order-item", "guestId": "guest-maya", "menuItemId": "salmon-carrot" }),
                ),
            ),
        )
        .await;
    }

    // Still a draft: nothing is committed until the ticket is fired.
    let (_, before) = send(&app, get("/api/recommendations/guest-maya")).await;
    let salmon_before = before["dishes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|dish| dish["id"] == "salmon-carrot")
        .unwrap()
        .clone();
    assert_eq!(salmon_before["eligible"], true);

    send(
        &app,
        post(
            "/api/actions",
            step("a5", json!({ "type": "send-order", "guestId": "guest-maya" })),
        ),
    )
    .await;

    let (_, state) = send(&app, get("/api/state")).await;
    let carrots = state["state"]["ingredients"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == "carrot")
        .unwrap()
        .clone();
    assert_eq!(carrots["onHand"], 0.0);

    let (_, after) = send(&app, get("/api/recommendations/guest-maya")).await;
    let salmon_after = after["dishes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|dish| dish["id"] == "salmon-carrot")
        .unwrap()
        .clone();
    assert_eq!(
        salmon_after["eligible"], false,
        "the dish must go dark once its stock is committed"
    );
    assert_eq!(salmon_after["score"], 0.0);
}

#[tokio::test]
async fn health_reports_which_integrations_are_configured() {
    let app = app();
    let (status, body) = send(&app, get("/api/health")).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["ok"], true);
    assert_eq!(body["elevenlabs"], false);
    assert_eq!(body["tavily"], false);
    assert_eq!(body["actionsLogged"], 0);
}

// --- sponsor guards over HTTP ---------------------------------------------

#[tokio::test]
async fn a_demo_session_is_issued_with_a_hardened_cookie() {
    let app = app();
    let response = app
        .clone()
        .oneshot(post("/api/demo-session", json!({})))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let cookie = response
        .headers()
        .get("set-cookie")
        .expect("a session cookie")
        .to_str()
        .unwrap()
        .to_string();
    assert!(cookie.contains("HttpOnly"));
    assert!(cookie.contains("SameSite=Strict"));
    assert!(
        !cookie.contains("Secure"),
        "the default config serves plain http, where a Secure cookie would be dropped"
    );
}

#[tokio::test]
async fn sponsor_routes_require_a_demo_session() {
    let app = app();

    let (status, _) = send(&app, get("/api/elevenlabs/token")).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, _) = send(&app, post("/api/tavily/search", json!({ "dishId": "beet-salad" }))).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn sponsor_routes_reject_cross_site_requests() {
    let app = app();
    let request = Request::builder()
        .uri("/api/elevenlabs/token")
        .header("host", "localhost:4000")
        .header("origin", "https://attacker.example")
        .header("sec-fetch-site", "cross-site")
        .header("cookie", SESSION)
        .body(Body::empty())
        .unwrap();

    let (status, _) = send(&app, request).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn an_unconfigured_elevenlabs_key_degrades_to_typing() {
    let app = app();
    let request = Request::builder()
        .uri("/api/elevenlabs/token")
        .header("host", "localhost:4000")
        .header("cookie", SESSION)
        .body(Body::empty())
        .unwrap();

    let (status, body) = send(&app, request).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["configured"], false);
}

#[tokio::test]
async fn an_unconfigured_tavily_key_returns_the_seeded_fallback() {
    let app = app();
    let request = Request::builder()
        .method("POST")
        .uri("/api/tavily/search")
        .header("host", "localhost:4000")
        .header("content-type", "application/json")
        .header("cookie", SESSION)
        .body(Body::from(json!({ "dishId": "beet-salad" }).to_string()))
        .unwrap();

    let (status, body) = send(&app, request).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["isFallback"], true);
    assert!(body["sources"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn an_unknown_dish_is_a_404() {
    let app = app();
    let request = Request::builder()
        .method("POST")
        .uri("/api/tavily/search")
        .header("host", "localhost:4000")
        .header("content-type", "application/json")
        .header("cookie", SESSION)
        .body(Body::from(json!({ "dishId": "no-such-dish" }).to_string()))
        .unwrap();

    let (status, _) = send(&app, request).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// --- live updates ---------------------------------------------------------

#[tokio::test]
async fn the_event_stream_opens_with_the_current_state_and_then_pushes_changes() {
    let state = AppState::new(Config::default()).unwrap();
    let app = ember_server::router(state.clone());

    let response = app
        .clone()
        .oneshot(get("/api/stream"))
        .await
        .expect("stream opens");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .map(|value| value.starts_with("text/event-stream")),
        Some(true)
    );

    let mut body = response.into_body().into_data_stream();

    // The first frame is the snapshot a client needs to render immediately.
    let first = next_event(&mut body).await;
    assert!(first.contains("\"version\":0"), "{first}");

    // Apply an action through the API and expect it to arrive on the stream.
    let seated = app
        .clone()
        .oneshot(post("/api/actions", seat("a1", "guest-maya", "t2")))
        .await
        .unwrap();
    assert_eq!(seated.status(), StatusCode::OK);

    let pushed = next_event(&mut body).await;
    assert!(pushed.contains("\"version\":1"), "{pushed}");
    assert!(pushed.contains("guest-maya"), "{pushed}");
}

async fn next_event(
    stream: &mut axum::body::BodyDataStream,
) -> String {
    use futures::StreamExt;

    let frame = tokio::time::timeout(std::time::Duration::from_secs(5), stream.next())
        .await
        .expect("an event within five seconds")
        .expect("the stream is still open")
        .expect("a readable frame");
    String::from_utf8_lossy(&frame).to_string()
}

// --- a whole service ------------------------------------------------------

#[tokio::test]
async fn a_full_service_runs_from_arrival_to_sent_order() {
    let app = app();

    let step = |id: &str, kind: Value| {
        let mut action = json!({ "id": id, "at": "2026-08-13T10:00:00.000Z" });
        action.as_object_mut().unwrap().extend(
            kind.as_object().unwrap().clone(),
        );
        action
    };

    // Check Jordan in, seat them, build an order, fire it.
    let (_, body) = send(
        &app,
        post(
            "/api/actions",
            step("a1", json!({ "type": "check-in", "guestId": "guest-jordan" })),
        ),
    )
    .await;
    assert_eq!(body["outcome"], "changed");

    let (_, body) = send(&app, post("/api/actions", seat("a2", "guest-jordan", "t7"))).await;
    assert_eq!(body["outcome"], "changed");

    let (_, body) = send(
        &app,
        post(
            "/api/actions",
            step(
                "a3",
                json!({ "type": "add-order-item", "guestId": "guest-jordan", "menuItemId": "beet-salad" }),
            ),
        ),
    )
    .await;
    assert_eq!(body["outcome"], "changed");

    let (_, body) = send(
        &app,
        post(
            "/api/actions",
            step("a4", json!({ "type": "send-order", "guestId": "guest-jordan" })),
        ),
    )
    .await;
    assert_eq!(body["outcome"], "changed");
    assert_eq!(body["version"], 4);

    let jordan = body["state"]["guests"]
        .as_array()
        .unwrap()
        .iter()
        .find(|guest| guest["id"] == "guest-jordan")
        .unwrap()
        .clone();
    assert_eq!(jordan["status"], "ordered");

    // Every step is on the audit log.
    let (_, health) = send(&app, get("/api/health")).await;
    assert_eq!(health["actionsLogged"], 4);
}
