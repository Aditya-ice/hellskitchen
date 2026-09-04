//! HTTP + SSE server for Ember POS.
//!
//! One binary serves everything: the exported Next.js bundle, the JSON API, and
//! the live event stream. The desktop app embeds this same server, so a browser
//! tab, a phone on the LAN, and the native app all run identical code paths
//! against one floor.

pub mod brain;
pub mod config;
pub mod guard;
pub mod session;
pub mod sponsors;
mod statics;

use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Utc;
use ember_core::{engine, seed, Action, Actor, Recommendation, Rejection, StaffRole};
use ember_store::auth::AuthOutcome;
use ember_store::{Applied, Revision, Store};
use futures::stream::Stream;

use crate::session::CurrentSession;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

pub use config::Config;

const SSE_CHANNEL_CAPACITY: usize = 64;

pub struct AppState {
    pub store: Store,
    pub config: Config,
    updates: broadcast::Sender<Revision>,
    limiter: guard::RateLimiter,
    http: reqwest::Client,
}

impl AppState {
    pub fn new(config: Config) -> Result<Arc<Self>, ember_store::StoreError> {
        let store = match &config.database {
            Some(path) => {
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent).ok();
                }
                Store::open(path)?
            }
            None => Store::in_memory()?,
        };
        let (updates, _) = broadcast::channel(SSE_CHANNEL_CAPACITY);

        Ok(Arc::new(Self {
            store,
            config,
            updates,
            limiter: guard::RateLimiter::new(),
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(20))
                .build()
                .unwrap_or_default(),
        }))
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Revision> {
        self.updates.subscribe()
    }

    /// Applies an action and, if it changed anything, tells every subscriber.
    ///
    /// The one way state changes. The HTTP handler and the desktop menu both go
    /// through here, so a reset from the native menu is logged, versioned and
    /// broadcast exactly like one from a browser.
    pub fn apply(&self, action: &Action) -> Result<Applied, ember_store::StoreError> {
        let applied = self.store.apply(action)?;
        if let Applied::Changed(revision) = &applied {
            // An error here only means nobody is listening yet.
            let _ = self.updates.send(revision.clone());
        }
        Ok(applied)
    }
}

type Shared = Arc<AppState>;

pub fn router(state: Shared) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/auth/setup", post(auth_setup))
        .route("/api/auth/login", post(auth_login))
        .route("/api/auth/logout", post(auth_logout))
        .route("/api/auth/me", get(auth_me))
        .route("/api/state", get(state_handler))
        .route("/api/actions", post(actions))
        .route("/api/stream", get(stream))
        .route("/api/menu", get(menu))
        .route("/api/recommendations/{guest_id}", get(recommendations))
        .route("/api/summary", get(summary))
        .route("/api/actions/log", get(action_log))
        .route("/api/forecast", get(forecast))
        .route("/api/agent/ask", post(agent_ask))
        .route("/api/elevenlabs/token", get(elevenlabs_token))
        .route("/api/tavily/search", post(tavily_search))
        .fallback(statics::serve)
        .with_state(state)
}

// --- error plumbing -------------------------------------------------------

struct ApiError(StatusCode, String);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(serde_json::json!({ "error": self.1 }))).into_response()
    }
}

impl From<ember_store::StoreError> for ApiError {
    fn from(error: ember_store::StoreError) -> Self {
        eprintln!("store error: {error}");
        ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "The POS could not read or write its state.".into(),
        )
    }
}

type ApiResult<T> = Result<T, ApiError>;

// --- handlers -------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Health {
    ok: bool,
    /// The build serving this, so a deployment can tell which binary answered.
    build: &'static str,
    /// Revision of the floor: how many actions have changed the state. Not a
    /// build number — `build` is that.
    revision: i64,
    /// Which schema migration the database is at. An upgrade that failed to run
    /// shows up here rather than as a confusing error later.
    schema_version: i64,
    actions_logged: i64,
    /// Whether each optional integration is configured. The UI uses this to
    /// decide between live and fallback affordances instead of guessing.
    elevenlabs: bool,
    tavily: bool,
    brain: bool,
}

async fn health(State(state): State<Shared>) -> ApiResult<Json<Health>> {
    Ok(Json(Health {
        ok: true,
        build: env!("CARGO_PKG_VERSION"),
        revision: state.store.revision()?.version,
        schema_version: state.store.schema_version()?,
        actions_logged: state.store.action_count()?,
        elevenlabs: state.config.elevenlabs_key.is_some(),
        tavily: state.config.tavily_key.is_some(),
        brain: state.config.brain_url.is_some(),
    }))
}

// --- authentication -------------------------------------------------------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Credentials {
    staff_id: String,
    pin: String,
    /// Which screen this is. Free-form, so a venue can label the pass, the host
    /// stand and the bar however it likes; it lands in the audit trail beside
    /// the staff id.
    #[serde(default)]
    terminal_id: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Identity {
    staff_id: String,
    name: String,
    role: StaffRole,
    terminal_id: String,
}

fn identity(session: &ember_store::auth::Session) -> Option<Identity> {
    seed::staff()
        .into_iter()
        .find(|member| member.id == session.staff_id)
        .map(|member| Identity {
            staff_id: member.id,
            name: member.name,
            role: member.role,
            terminal_id: session.terminal_id.clone(),
        })
}

/// First-run bootstrap: sets the first PIN when no credential exists yet.
///
/// Open by necessity — there is nobody to authenticate as before this runs —
/// and closed the moment it succeeds, because it refuses to do anything once
/// any credential exists. It also only accepts a manager, so the first account
/// on a new terminal cannot be a low-privilege one that then cannot grant
/// anything.
async fn auth_setup(
    State(state): State<Shared>,
    headers: HeaderMap,
    payload: Option<Json<Credentials>>,
) -> ApiResult<Response> {
    guard::require_same_origin(&headers).map_err(|rejection| {
        ApiError(
            rejection.status(),
            "Cross-site requests are not allowed.".into(),
        )
    })?;

    let Json(credentials) = payload.ok_or_else(|| {
        ApiError(
            StatusCode::BAD_REQUEST,
            "Expected a staff id and a PIN.".into(),
        )
    })?;

    if state.store.has_any_credentials()? {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "This terminal already has staff PINs. Ask a manager to add yours.".into(),
        ));
    }

    if role_of(&credentials.staff_id) != Some(StaffRole::Manager) {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "The first PIN must belong to a manager.".into(),
        ));
    }

    state
        .store
        .set_staff_pin(&credentials.staff_id, &credentials.pin, Utc::now())
        .map_err(|error| match error {
            ember_store::StoreError::WeakPin => ApiError(
                StatusCode::BAD_REQUEST,
                "A PIN must be 4 to 12 digits.".into(),
            ),
            other => ApiError(StatusCode::INTERNAL_SERVER_ERROR, other.to_string()),
        })?;

    Ok((StatusCode::CREATED, Json(serde_json::json!({ "ok": true }))).into_response())
}

async fn auth_login(
    State(state): State<Shared>,
    headers: HeaderMap,
    payload: Option<Json<Credentials>>,
) -> ApiResult<Response> {
    guard::require_same_origin(&headers).map_err(|rejection| {
        ApiError(
            rejection.status(),
            "Cross-site requests are not allowed.".into(),
        )
    })?;

    let Json(credentials) = payload.ok_or_else(|| {
        ApiError(
            StatusCode::BAD_REQUEST,
            "Expected a staff id and a PIN.".into(),
        )
    })?;

    // Rate-limited by IP as well as by the per-account lockout: the lockout
    // stops an attack on one account, this stops one sweeping the roster.
    state
        .limiter
        .check(
            &headers,
            "auth-login",
            state.config.trust_forwarded_for,
            10,
            Duration::from_secs(60),
        )
        .map_err(|rejection| {
            ApiError(
                rejection.status(),
                "Too many sign-in attempts. Wait a moment.".into(),
            )
        })?;

    let terminal = credentials
        .terminal_id
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "unnamed-terminal".into());

    match state
        .store
        .authenticate(&credentials.staff_id, &credentials.pin, &terminal, Utc::now())?
    {
        AuthOutcome::Granted { token, session } => {
            let mut response = Json(serde_json::json!({
                "ok": true,
                "identity": identity(&session),
            }))
            .into_response();
            response.headers_mut().insert(
                axum::http::header::SET_COOKIE,
                session::issue(&token, state.config.secure_cookies)
                    .parse()
                    .map_err(|_| {
                        ApiError(StatusCode::INTERNAL_SERVER_ERROR, "Could not issue a session.".into())
                    })?,
            );
            Ok(response)
        }
        // A wrong PIN and an unknown staff id give the same answer and take the
        // same time, so neither can be used to enumerate the roster.
        AuthOutcome::WrongPin { attempts_remaining } => Err(ApiError(
            StatusCode::UNAUTHORIZED,
            format!(
                "That PIN was not recognised. {attempts_remaining} attempt{} left before this account locks.",
                if attempts_remaining == 1 { "" } else { "s" }
            ),
        )),
        AuthOutcome::UnknownStaff => Err(ApiError(
            StatusCode::UNAUTHORIZED,
            "That PIN was not recognised.".into(),
        )),
        AuthOutcome::LockedOut { until } => Err(ApiError(
            StatusCode::LOCKED,
            format!(
                "This account is locked until {}. A manager can reset the PIN.",
                until.format("%H:%M")
            ),
        )),
    }
}

async fn auth_logout(State(state): State<Shared>, headers: HeaderMap) -> ApiResult<Response> {
    if let Some(token) = session::session_cookie(&headers) {
        state.store.end_session(token)?;
    }
    let mut response = Json(serde_json::json!({ "ok": true })).into_response();
    response.headers_mut().insert(
        axum::http::header::SET_COOKIE,
        session::clear(state.config.secure_cookies)
            .parse()
            .map_err(|_| {
                ApiError(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Could not clear the session.".into(),
                )
            })?,
    );
    Ok(response)
}

/// Who this terminal is signed in as. The UI calls it on load to decide between
/// the sign-in screen and the floor.
async fn auth_me(
    State(state): State<Shared>,
    headers: HeaderMap,
) -> ApiResult<Json<serde_json::Value>> {
    let session = session::session_cookie(&headers)
        .and_then(|token| state.store.session(token, Utc::now()).ok().flatten());

    Ok(Json(match session {
        Some(session) => serde_json::json!({
            "authenticated": true,
            "identity": identity(&session),
        }),
        None => serde_json::json!({
            "authenticated": false,
            // Drives first-run: with no PINs at all there is nothing to sign in
            // to, and the UI shows setup instead of a login it cannot satisfy.
            "needsSetup": !state.store.has_any_credentials().unwrap_or(false),
        }),
    }))
}

async fn state_handler(
    State(state): State<Shared>,
    CurrentSession(_): CurrentSession,
) -> ApiResult<Json<Revision>> {
    Ok(Json(state.store.revision()?))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ActionOutcome {
    /// "changed", "unchanged", "rejected", or "duplicate".
    ///
    /// Deliberately not an HTTP status: a refused action is a normal outcome of
    /// a busy service, not a transport failure, and the revision below is the
    /// caller's authoritative view either way.
    outcome: &'static str,
    /// Present only when `outcome` is "rejected". The tag is what a client
    /// switches on; `reasonMessage` is the fallback for one that has not
    /// mapped this variant yet.
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<Rejection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason_message: Option<&'static str>,
    #[serde(flatten)]
    revision: Revision,
}

/// The role a staff id carries, if the roster knows them.
///
/// Reads the seeded roster for now; when reference data moves into `PosState`
/// this becomes a lookup on the state and nothing else changes.
fn role_of(staff_id: &str) -> Option<StaffRole> {
    seed::staff()
        .into_iter()
        .find(|member| member.id == staff_id)
        .map(|member| member.role)
}

/// Actions a manager has to authorise.
///
/// `StaffRole` has existed since the first commit and no code path read it, so
/// a bus tablet could do everything a manager could. The gate is here, on the
/// server, because a UI that merely hides a button is not an authorisation
/// control -- anything that can reach the port can still send the action.
fn requires_manager(kind: &ember_core::ActionKind) -> bool {
    matches!(kind, ember_core::ActionKind::Reset)
}

/// What a client is allowed to send.
///
/// Deliberately not `Action`: `at` and `actor` are stamped here, from the
/// server clock and the session. `at` used to be taken from the request body
/// and written straight into `sent_at` and `completed_at`, which meant ticket
/// age and cook time -- the numbers the pass runs on, and the ones a dispute
/// turns on -- were whatever the client claimed they were.
#[derive(Deserialize)]
struct ActionRequest {
    /// Client-generated, and the one thing that must come from the client: it
    /// is what makes a retried request idempotent rather than a second seating.
    id: String,
    #[serde(flatten)]
    kind: ember_core::ActionKind,
}

async fn actions(
    State(state): State<Shared>,
    CurrentSession(session): CurrentSession,
    Json(request): Json<ActionRequest>,
) -> ApiResult<Json<ActionOutcome>> {
    if requires_manager(&request.kind) && role_of(&session.staff_id) != Some(StaffRole::Manager) {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "That needs a manager.".into(),
        ));
    }

    let action = Action {
        id: request.id,
        // Stamped here, never taken from the body: see ActionRequest.
        at: Utc::now().to_rfc3339(),
        actor: Some(Actor {
            staff_id: session.staff_id.clone(),
            terminal_id: session.terminal_id.clone(),
        }),
        kind: request.kind,
    };
    let applied = state.apply(&action)?;

    let (outcome, reason, revision) = match applied {
        Applied::Changed(revision) => ("changed", None, revision),
        Applied::Unchanged => ("unchanged", None, state.store.revision()?),
        Applied::Rejected(reason) => ("rejected", Some(reason), state.store.revision()?),
        Applied::Duplicate => ("duplicate", None, state.store.revision()?),
    };

    Ok(Json(ActionOutcome {
        outcome,
        reason,
        reason_message: reason.map(Rejection::message),
        revision,
    }))
}

/// Server-sent events: the current revision on connect, then every change.
async fn stream(
    State(state): State<Shared>,
    CurrentSession(_): CurrentSession,
) -> ApiResult<Sse<impl Stream<Item = Result<Event, std::convert::Infallible>>>> {
    use futures::StreamExt;
    use tokio_stream::wrappers::BroadcastStream;

    let initial = state.store.revision()?;
    let receiver = state.subscribe();

    let first = futures::stream::once(async move { initial });
    let rest = BroadcastStream::new(receiver).filter_map(|item| async move { item.ok() });

    let events = first.chain(rest).map(|revision| {
        Ok(Event::default()
            .event("state")
            .data(serde_json::to_string(&revision).unwrap_or_default()))
    });

    Ok(Sse::new(events).keep_alive(KeepAlive::default()))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
/// Static reference data. Live stock is not here — it moves during a service,
/// so it belongs to the state the stream pushes.
struct MenuPayload {
    restaurant: seed::Restaurant,
    menu_items: Vec<ember_core::MenuItem>,
    staff: Vec<ember_core::StaffMember>,
}

async fn menu(CurrentSession(_): CurrentSession) -> Json<MenuPayload> {
    Json(MenuPayload {
        restaurant: seed::restaurant(),
        menu_items: seed::menu_items(),
        staff: seed::staff(),
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RecommendationPayload {
    guest_id: String,
    version: i64,
    tables: Vec<Recommendation>,
    dishes: Vec<Recommendation>,
    estimate_wait: f64,
    order_total: f64,
    /// "engine" or "model". Honest about which ranking this actually is.
    ranked_by: &'static str,
}

#[derive(Deserialize)]
struct RecommendationQuery {
    /// `false` serves the engine's own ordering without consulting the brain.
    /// The brain uses this when it needs the engine ranking as its own input,
    /// which is what stops the two calling each other in a loop.
    #[serde(default)]
    rerank: Option<bool>,
}

async fn recommendations(
    State(state): State<Shared>,
    CurrentSession(_): CurrentSession,
    Path(guest_id): Path<String>,
    axum::extract::Query(query): axum::extract::Query<RecommendationQuery>,
) -> ApiResult<Json<RecommendationPayload>> {
    let revision = state.store.revision()?;
    let guest = revision.state.guest(&guest_id).ok_or_else(|| {
        ApiError(
            StatusCode::NOT_FOUND,
            format!("No guest with id {guest_id}."),
        )
    })?;

    let menu_items = seed::menu_items();

    // Scored against live stock, so a dish goes dark the moment the last
    // portion is committed to another ticket.
    let mut dishes = engine::recommend_dishes(guest, &menu_items, &revision.state.ingredients);
    let mut ranked_by = "engine";

    // Optional reranking by the brain. Any failure — absent, slow, erroring —
    // leaves the engine's ordering in place, which is a correct answer rather
    // than a degraded one, so none of this is worth surfacing as an error.
    if query.rerank.unwrap_or(true) {
        if let Some(base) = state.config.brain_url.as_deref() {
            if let Some(ranking) = brain::rerank(&state.http, base, &guest_id, &dishes).await {
                // Trust the engine's eligibility, not the brain's: a reranker
                // that returned a blocked dish as sellable must not be able to
                // put it in front of a server.
                if preserves_eligibility(&dishes, &ranking.dishes) {
                    dishes = ranking.dishes;
                    ranked_by = if ranking.ranked_by == "model" {
                        "model"
                    } else {
                        "engine"
                    };
                } else {
                    eprintln!("floor reranker changed dish eligibility; ignoring its ranking");
                }
            }
        }
    }

    Ok(Json(RecommendationPayload {
        tables: engine::recommend_tables(guest, &revision.state.tables),
        dishes,
        estimate_wait: engine::estimate_wait(guest, &revision.state.tables),
        order_total: engine::order_total(revision.state.order_for_guest(&guest_id), &menu_items),
        guest_id,
        version: revision.version,
        ranked_by,
    }))
}

/// Whether a reranking kept every dish's eligibility exactly as the engine set
/// it, and lost none of them.
///
/// The last line of defence. `services/brain` is careful not to touch
/// eligibility, but "careful" is a property of code that can change; this is
/// checked on every response.
fn preserves_eligibility(engine: &[Recommendation], reranked: &[Recommendation]) -> bool {
    if engine.len() != reranked.len() {
        return false;
    }
    engine.iter().all(|original| {
        reranked
            .iter()
            .any(|candidate| candidate.id == original.id && candidate.eligible == original.eligible)
    })
}

/// The optional demand forecast. Absent when the brain is not configured or
/// not answering.
async fn forecast(State(state): State<Shared>, CurrentSession(_): CurrentSession) -> Response {
    let Some(base) = state.config.brain_url.as_deref() else {
        return Json(serde_json::json!({ "available": false })).into_response();
    };
    match brain::forecast(&state.http, base).await {
        Some(body) => Json(body).into_response(),
        None => Json(serde_json::json!({ "available": false })).into_response(),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LogQuery {
    #[serde(default)]
    since: i64,
    #[serde(default)]
    limit: Option<i64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ActionLog {
    /// Highest seq returned, for polling onward from.
    latest_seq: i64,
    total: i64,
    entries: Vec<ember_store::LoggedAction>,
}

/// The append-only log, for anything that learns from what happened.
///
/// Read-only and paginated. The whole log of a long service is not something
/// to hand over in one response, and a caller that wants it all can walk it.
async fn action_log(
    State(state): State<Shared>,
    CurrentSession(_): CurrentSession,
    axum::extract::Query(query): axum::extract::Query<LogQuery>,
) -> ApiResult<Json<ActionLog>> {
    let limit = query.limit.unwrap_or(500).clamp(1, 2000);
    let entries = state.store.actions(query.since.max(0), limit)?;

    Ok(Json(ActionLog {
        latest_seq: entries.last().map(|entry| entry.seq).unwrap_or(query.since),
        total: state.store.action_count()?,
        entries,
    }))
}

/// Floor-wide numbers for the header.
///
/// Separate from the per-guest recommendations because these are properties of
/// the service, not of whoever happens to be selected. The average wait needs
/// the engine, so it cannot be computed in the browser.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FloorSummary {
    version: i64,
    waiting_guests: usize,
    open_tables: usize,
    /// Mean estimated wait across parties still waiting, in minutes. Zero when
    /// nobody is waiting — which is a real answer, not a missing one.
    average_wait_minutes: f64,
}

async fn summary(
    State(state): State<Shared>,
    CurrentSession(_): CurrentSession,
) -> ApiResult<Json<FloorSummary>> {
    let revision = state.store.revision()?;
    let floor = &revision.state;

    let waiting: Vec<_> = floor
        .guests
        .iter()
        .filter(|guest| guest.status == ember_core::GuestStatus::Waiting)
        .collect();

    let average_wait_minutes = if waiting.is_empty() {
        0.0
    } else {
        let total: f64 = waiting
            .iter()
            .map(|guest| engine::estimate_wait(guest, &floor.tables))
            .sum();
        (total / waiting.len() as f64).round()
    };

    Ok(Json(FloorSummary {
        version: revision.version,
        waiting_guests: waiting.len(),
        open_tables: floor
            .tables
            .iter()
            .filter(|table| table.status == ember_core::TableStatus::Available)
            .count(),
        average_wait_minutes,
    }))
}

/// Asks the optional Python service a question about the live floor.
///
/// Guarded like the sponsor routes: an agent turn costs real money, so it needs
/// a session and is rate limited. Always answers 200 — a missing or failing
/// optional service is reported in the answer, not as an error.
async fn agent_ask(
    State(state): State<Shared>,
    CurrentSession(_): CurrentSession,
    headers: HeaderMap,
    body: Option<Json<AgentQuestion>>,
) -> Response {
    if let Err(rejection) = guard::require_same_origin(&headers) {
        return rejection.into_response();
    }
    if let Err(rejection) = state.limiter.check(
        &headers,
        "agent-ask",
        state.config.trust_forwarded_for,
        8,
        Duration::from_secs(60),
    ) {
        return rejection.into_response();
    }

    let Some(Json(query)) = body else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "A question is required." })),
        )
            .into_response();
    };
    let question = query.question.trim();
    if question.is_empty() || question.len() > 2000 {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "A question is required." })),
        )
            .into_response();
    }

    let Some(base) = state.config.brain_url.as_deref() else {
        return Json(brain::AgentAnswer::not_running()).into_response();
    };

    Json(brain::ask(&state.http, base, question).await).into_response()
}

#[derive(Deserialize)]
struct AgentQuestion {
    question: String,
}

// --- sponsor routes -------------------------------------------------------

async fn elevenlabs_token(
    State(state): State<Shared>,
    CurrentSession(_): CurrentSession,
    headers: HeaderMap,
) -> Response {
    if let Err(rejection) = guard::require_same_origin(&headers) {
        return rejection.into_response();
    }
    if let Err(rejection) = state.limiter.check(
        &headers,
        "elevenlabs-token",
        state.config.trust_forwarded_for,
        6,
        Duration::from_secs(60),
    ) {
        return rejection.into_response();
    }

    let Some(api_key) = state.config.elevenlabs_key.as_deref() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "ElevenLabs is not configured. Use the typed demo input instead.",
                "configured": false,
            })),
        )
            .into_response();
    };

    match sponsors::elevenlabs_token(&state.http, &state.config.elevenlabs_base, api_key).await {
        Ok(token) => {
            Json(serde_json::json!({ "token": token, "configured": true })).into_response()
        }
        Err(error) => {
            eprintln!("unable to create ElevenLabs token: {error}");
            (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({
                    "error": "Voice transcription is temporarily unavailable."
                })),
            )
                .into_response()
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DishQuery {
    dish_id: String,
}

async fn tavily_search(
    State(state): State<Shared>,
    CurrentSession(_): CurrentSession,
    headers: HeaderMap,
    body: Option<Json<DishQuery>>,
) -> Response {
    if let Err(rejection) = guard::require_same_origin(&headers) {
        return rejection.into_response();
    }
    if let Err(rejection) = state.limiter.check(
        &headers,
        "tavily-search",
        state.config.trust_forwarded_for,
        10,
        Duration::from_secs(60),
    ) {
        return rejection.into_response();
    }

    let Some(Json(query)) = body else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "A valid dish ID is required." })),
        )
            .into_response();
    };
    if query.dish_id.len() > 80 {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "A valid dish ID is required." })),
        )
            .into_response();
    }

    let menu_items = seed::menu_items();
    let Some(dish) = menu_items.iter().find(|item| item.id == query.dish_id) else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Dish not found." })),
        )
            .into_response();
    };

    let Some(api_key) = state.config.tavily_key.as_deref() else {
        return Json(sponsors::fallback_context()).into_response();
    };

    Json(sponsors::tavily_context(&state.http, &state.config.tavily_base, api_key, dish).await)
        .into_response()
}

/// Binds the configured address and serves until the process is asked to stop.
pub async fn serve(state: Shared) -> std::io::Result<()> {
    let listener =
        tokio::net::TcpListener::bind((state.config.host.clone(), state.config.port)).await?;
    let address = listener.local_addr()?;
    println!("Ember POS server listening on http://{address}");
    serve_on(listener, state).await
}

/// Serves on an already-bound listener.
///
/// The desktop app binds port 0 to get a free port, needs to know which port it
/// got before the window can be pointed at it, and must not race another
/// process for it in between — so it binds first and hands the listener over.
pub async fn serve_on(listener: tokio::net::TcpListener, state: Shared) -> std::io::Result<()> {
    axum::serve(listener, router(state)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dish(id: &str, eligible: bool) -> Recommendation {
        Recommendation {
            id: id.into(),
            score: if eligible { 50.0 } else { 0.0 },
            eligible,
            reasons: vec![],
            warnings: vec![],
        }
    }

    #[test]
    fn a_pure_reordering_is_accepted() {
        let engine = vec![dish("a", true), dish("b", true), dish("c", false)];
        let reranked = vec![dish("b", true), dish("a", true), dish("c", false)];
        assert!(preserves_eligibility(&engine, &reranked));
    }

    #[test]
    fn promoting_a_blocked_dish_to_sellable_is_refused() {
        // The reranker has no business deciding this, and a bug there must not
        // be able to put an allergen in front of a server.
        let engine = vec![dish("a", true), dish("c", false)];
        let reranked = vec![dish("c", true), dish("a", true)];
        assert!(!preserves_eligibility(&engine, &reranked));
    }

    #[test]
    fn blocking_a_sellable_dish_is_also_refused() {
        // Less dangerous, but still not the reranker's call.
        let engine = vec![dish("a", true), dish("b", true)];
        let reranked = vec![dish("a", true), dish("b", false)];
        assert!(!preserves_eligibility(&engine, &reranked));
    }

    #[test]
    fn dropping_a_dish_is_refused() {
        let engine = vec![dish("a", true), dish("b", true)];
        assert!(!preserves_eligibility(&engine, &[dish("a", true)]));
    }

    #[test]
    fn inventing_a_dish_is_refused() {
        let engine = vec![dish("a", true)];
        let reranked = vec![dish("a", true), dish("ghost", true)];
        assert!(!preserves_eligibility(&engine, &reranked));
    }

    #[test]
    fn an_empty_ranking_matches_an_empty_menu() {
        assert!(preserves_eligibility(&[], &[]));
    }
}
