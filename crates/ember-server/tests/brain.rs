//! The optional brain: reranking and forecasting, against a stub service.
//!
//! Two properties matter more than the happy path. The POS must be identical
//! whether or not the brain is there, and a reranker that misbehaves must not
//! be able to change what a guest may be sold.

use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::extract::State;
use axum::http::{Request, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use ember_server::{AppState, Config};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt;

/// How the stub brain should behave.
#[derive(Clone, Copy, PartialEq)]
enum Behaviour {
    /// Reverses the engine's order, keeping eligibility intact.
    Reorder,
    /// Claims a blocked dish is sellable. Must be refused.
    FlipEligibility,
    /// Drops a dish. Must be refused.
    DropDish,
    /// Reports itself unavailable.
    Unavailable,
}

#[derive(Clone)]
struct Stub {
    behaviour: Behaviour,
    calls: Arc<Mutex<Vec<Value>>>,
}

async fn rank(State(stub): State<Stub>, request: Request<Body>) -> axum::response::Response {
    let bytes = request
        .into_body()
        .collect()
        .await
        .map(|body| body.to_bytes())
        .unwrap_or_default();
    let body: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    stub.calls.lock().unwrap().push(body.clone());

    if stub.behaviour == Behaviour::Unavailable {
        return Json(json!({ "available": false, "reason": "no" })).into_response();
    }

    let mut dishes: Vec<Value> = body["dishes"].as_array().cloned().unwrap_or_default();
    match stub.behaviour {
        Behaviour::Reorder => dishes.reverse(),
        Behaviour::FlipEligibility => {
            for dish in dishes.iter_mut() {
                dish["eligible"] = json!(true);
                dish["score"] = json!(99.0);
            }
            dishes.reverse();
        }
        Behaviour::DropDish => {
            dishes.truncate(1);
        }
        Behaviour::Unavailable => {}
    }

    Json(json!({ "available": true, "dishes": dishes, "rankedBy": "model", "ticketsSeen": 7 }))
        .into_response()
}

async fn forecast(State(stub): State<Stub>) -> axum::response::Response {
    if stub.behaviour == Behaviour::Unavailable {
        return Json(json!({ "available": false })).into_response();
    }
    Json(json!({
        "available": true,
        "confidence": "fair",
        "actionable": true,
        "stockoutRisks": [
            { "ingredientId": "carrot", "name": "Carrots", "minutesToZero": 34,
              "blocks": ["Cedar Salmon"], "onHand": 2.0, "unit": "lb", "burnPerHour": 3.5 }
        ],
        "covers": { "partiesSeated": 4 }
    }))
    .into_response()
}

async fn stub_brain(behaviour: Behaviour) -> (String, Arc<Mutex<Vec<Value>>>) {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let app = Router::new()
        .route("/rank", post(rank))
        .route("/forecast", get(forecast))
        .with_state(Stub {
            behaviour,
            calls: calls.clone(),
        });

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    (format!("http://{address}"), calls)
}

fn app_with(base: Option<&str>) -> axum::Router {
    ember_server::router(
        AppState::new(Config {
            brain_url: base.map(str::to_string),
            ..Config::default()
        })
        .expect("in-memory store"),
    )
}

async fn send(app: &axum::Router, uri: &str) -> (StatusCode, Value) {
    let request = Request::builder()
        .uri(uri)
        .header("host", "localhost:4000")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

fn ids(body: &Value) -> Vec<String> {
    body["dishes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|dish| dish["id"].as_str().unwrap().to_string())
        .collect()
}

// --- reranking ------------------------------------------------------------

#[tokio::test]
async fn without_a_brain_the_engine_ranking_is_served() {
    let app = app_with(None);
    let (status, body) = send(&app, "/api/recommendations/guest-maya").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["rankedBy"], "engine");
    assert_eq!(body["dishes"][0]["id"], "beet-salad");
}

#[tokio::test]
async fn a_brain_reranking_is_applied_and_labelled() {
    let (base, calls) = stub_brain(Behaviour::Reorder).await;
    let app = app_with(Some(&base));

    let (_, engine) = send(&app, "/api/recommendations/guest-maya?rerank=false").await;
    let (_, reranked) = send(&app, "/api/recommendations/guest-maya").await;

    assert_eq!(engine["rankedBy"], "engine");
    assert_eq!(reranked["rankedBy"], "model");

    let mut expected = ids(&engine);
    expected.reverse();
    assert_eq!(ids(&reranked), expected);

    // The engine's ranking travels in the request, so the brain never has to
    // fetch it back — which would re-enter this endpoint and recurse.
    let sent = calls.lock().unwrap();
    assert_eq!(sent[0]["guestId"], "guest-maya");
    assert!(sent[0]["dishes"].as_array().unwrap().len() > 1);
}

#[tokio::test]
async fn rerank_false_never_consults_the_brain() {
    // This is what breaks the loop, so it is worth pinning.
    let (base, calls) = stub_brain(Behaviour::Reorder).await;
    let app = app_with(Some(&base));

    let (_, body) = send(&app, "/api/recommendations/guest-maya?rerank=false").await;

    assert_eq!(body["rankedBy"], "engine");
    assert!(calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_reranker_that_unblocks_a_dish_is_ignored_entirely() {
    // The whole reason the model sits outside the engine. Maya has a tree-nut
    // allergy; no ranking may make the tartare sellable.
    let (base, _) = stub_brain(Behaviour::FlipEligibility).await;
    let app = app_with(Some(&base));

    let (_, body) = send(&app, "/api/recommendations/guest-maya").await;

    assert_eq!(
        body["rankedBy"], "engine",
        "the bad ranking must be discarded"
    );
    let tartare = body["dishes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|dish| dish["id"] == "carrot-tartare")
        .unwrap();
    assert_eq!(tartare["eligible"], false);
    assert_eq!(tartare["score"], 0.0);
    assert!(tartare["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|warning| warning == "Contains guest allergen: tree nuts"));
}

#[tokio::test]
async fn a_reranker_that_loses_a_dish_is_ignored() {
    let (base, _) = stub_brain(Behaviour::DropDish).await;
    let app = app_with(Some(&base));

    let (_, body) = send(&app, "/api/recommendations/guest-maya").await;

    assert_eq!(body["rankedBy"], "engine");
    assert_eq!(body["dishes"].as_array().unwrap().len(), 9);
}

#[tokio::test]
async fn a_brain_reporting_itself_unavailable_falls_back_quietly() {
    let (base, _) = stub_brain(Behaviour::Unavailable).await;
    let app = app_with(Some(&base));

    let (status, body) = send(&app, "/api/recommendations/guest-maya").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["rankedBy"], "engine");
}

#[tokio::test]
async fn an_unreachable_brain_falls_back_quietly() {
    let app = app_with(Some("http://127.0.0.1:1"));
    let (status, body) = send(&app, "/api/recommendations/guest-maya").await;

    assert_eq!(status, StatusCode::OK, "a missing brain is not an error");
    assert_eq!(body["rankedBy"], "engine");
    assert_eq!(body["dishes"].as_array().unwrap().len(), 9);
}

// --- forecasting ----------------------------------------------------------

#[tokio::test]
async fn the_forecast_is_served_when_the_brain_offers_one() {
    let (base, _) = stub_brain(Behaviour::Reorder).await;
    let app = app_with(Some(&base));

    let (status, body) = send(&app, "/api/forecast").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["available"], true);
    assert_eq!(body["stockoutRisks"][0]["name"], "Carrots");
    assert_eq!(body["stockoutRisks"][0]["minutesToZero"], 34);
}

#[tokio::test]
async fn no_brain_means_no_forecast_rather_than_an_error() {
    let app = app_with(None);
    let (status, body) = send(&app, "/api/forecast").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["available"], false);
}

#[tokio::test]
async fn an_unreachable_brain_means_no_forecast() {
    let app = app_with(Some("http://127.0.0.1:1"));
    let (status, body) = send(&app, "/api/forecast").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["available"], false);
}
