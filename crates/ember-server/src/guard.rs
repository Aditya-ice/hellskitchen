//! Request guards for the sponsor-integration routes.
//!
//! Ported from `lib/api-guard.ts`. The behaviour is the same; the tests are not.
//! The `/code-review` pass found that three of the original vitest cases could
//! not fail — the same-origin case short-circuited before reaching the host
//! comparison and never asserted the allow path, the rate-limit case reused one
//! identity so a global limiter would have passed it, and the cookie case could
//! not observe `Secure` because that branch only existed under
//! `NODE_ENV=production`. Here `Secure` is an explicit config value, so both
//! branches are reachable, and each guard is tested for what it lets through as
//! well as what it blocks.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;

pub const SESSION_COOKIE: &str = "ember_demo_session";

/// Why a request was refused, and what the client should be told.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rejection {
    CrossSite,
    InvalidOrigin,
    NoSession,
    RateLimited { retry_after_secs: u64 },
}

impl Rejection {
    fn parts(&self) -> (StatusCode, &'static str) {
        match self {
            Rejection::CrossSite => (
                StatusCode::FORBIDDEN,
                "Cross-site requests are not allowed.",
            ),
            Rejection::InvalidOrigin => (StatusCode::FORBIDDEN, "Invalid request origin."),
            Rejection::NoSession => (
                StatusCode::UNAUTHORIZED,
                "Start a demo session before using sponsor integrations.",
            ),
            Rejection::RateLimited { .. } => (
                StatusCode::TOO_MANY_REQUESTS,
                "Too many requests. Please try again shortly.",
            ),
        }
    }

    pub fn status(&self) -> StatusCode {
        self.parts().0
    }
}

impl IntoResponse for Rejection {
    fn into_response(self) -> Response {
        let (status, message) = self.parts();
        let body = Json(serde_json::json!({ "error": message }));
        match self {
            Rejection::RateLimited { retry_after_secs } => (
                status,
                [("Retry-After", retry_after_secs.to_string())],
                body,
            )
                .into_response(),
            _ => (status, body).into_response(),
        }
    }
}

fn header<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name).and_then(|value| value.to_str().ok())
}

/// Reads one cookie out of the `Cookie` header.
fn cookie<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    header(headers, "cookie")?.split(';').find_map(|part| {
        let (key, value) = part.split_once('=')?;
        (key.trim() == name).then_some(value.trim())
    })
}

/// A demo session id is a UUID. Anything else is treated as absent.
fn is_session_id(value: &str) -> bool {
    value.len() == 36
        && value
            .chars()
            .all(|character| character.is_ascii_hexdigit() || character == '-')
}

pub fn demo_session(headers: &HeaderMap) -> Option<&str> {
    cookie(headers, SESSION_COOKIE).filter(|value| is_session_id(value))
}

pub fn require_demo_session(headers: &HeaderMap) -> Result<&str, Rejection> {
    demo_session(headers).ok_or(Rejection::NoSession)
}

/// Rejects requests that a browser tells us came from another site, and
/// requests whose `Origin` does not match the host they were sent to.
///
/// `host` is the value of the `Host` header — the Rust equivalent of reading
/// `new URL(request.url).host` in the Next.js route handler.
pub fn require_same_origin(headers: &HeaderMap) -> Result<(), Rejection> {
    if header(headers, "sec-fetch-site") == Some("cross-site") {
        return Err(Rejection::CrossSite);
    }

    let Some(origin) = header(headers, "origin") else {
        // No Origin header: a same-origin GET, or a non-browser client.
        return Ok(());
    };

    let origin_host = origin
        .split_once("://")
        .map(|(_, rest)| rest.trim_end_matches('/'))
        .ok_or(Rejection::InvalidOrigin)?;
    if origin_host.is_empty() {
        return Err(Rejection::InvalidOrigin);
    }

    match header(headers, "host") {
        Some(host) if host == origin_host => Ok(()),
        _ => Err(Rejection::InvalidOrigin),
    }
}

fn client_ip(headers: &HeaderMap) -> &str {
    header(headers, "x-forwarded-for")
        .and_then(|value| value.split(',').next())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| header(headers, "x-real-ip"))
        .unwrap_or("local")
}

/// Sliding-window limiter, bucketed per scope and per caller identity.
///
/// The identity half matters: without it one visitor exhausting a quota would
/// lock out everyone else. `limits_each_identity_separately` pins that.
#[derive(Default)]
pub struct RateLimiter {
    buckets: Mutex<HashMap<String, Vec<Instant>>>,
}

impl RateLimiter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn check(
        &self,
        headers: &HeaderMap,
        scope: &str,
        limit: usize,
        window: Duration,
    ) -> Result<(), Rejection> {
        let identity = format!(
            "{}:{}",
            client_ip(headers),
            demo_session(headers).unwrap_or("anonymous")
        );
        let key = format!("{scope}:{identity}");
        let now = Instant::now();

        let mut buckets = match self.buckets.lock() {
            Ok(guard) => guard,
            // A poisoned lock must not become an open door.
            Err(poisoned) => poisoned.into_inner(),
        };
        let bucket = buckets.entry(key).or_default();
        bucket.retain(|stamp| now.duration_since(*stamp) < window);

        if bucket.len() >= limit {
            let oldest = bucket[0];
            let retry_after_secs = window
                .saturating_sub(now.duration_since(oldest))
                .as_secs()
                .max(1);
            return Err(Rejection::RateLimited { retry_after_secs });
        }

        bucket.push(now);
        Ok(())
    }
}

/// Builds the `Set-Cookie` value for a new demo session.
///
/// `secure` is passed in rather than read from the environment so that both
/// branches are reachable from a test.
pub fn session_cookie(value: &str, secure: bool) -> String {
    format!(
        "{SESSION_COOKIE}={value}; Path=/; HttpOnly; SameSite=Strict; Max-Age=3600{}",
        if secure { "; Secure" } else { "" }
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in pairs {
            map.insert(
                axum::http::HeaderName::from_bytes(name.as_bytes()).unwrap(),
                value.parse().unwrap(),
            );
        }
        map
    }

    // --- same-origin ---
    //
    // Split into three cases. The original test set both `sec-fetch-site:
    // cross-site` and a foreign `Origin`, so it returned at the first check and
    // the host comparison below was never executed by any test.

    #[test]
    fn rejects_a_cross_site_fetch_on_the_header_alone() {
        let request = headers(&[("sec-fetch-site", "cross-site"), ("host", "localhost:4000")]);
        assert_eq!(require_same_origin(&request), Err(Rejection::CrossSite));
    }

    #[test]
    fn rejects_a_foreign_origin_on_the_host_comparison_alone() {
        // No sec-fetch-site header, so this can only be caught by comparing
        // Origin against Host.
        let request = headers(&[
            ("origin", "https://attacker.example"),
            ("host", "localhost:4000"),
        ]);
        assert_eq!(require_same_origin(&request), Err(Rejection::InvalidOrigin));
    }

    #[test]
    fn allows_a_legitimate_same_origin_request() {
        let request = headers(&[
            ("origin", "http://localhost:4000"),
            ("host", "localhost:4000"),
            ("sec-fetch-site", "same-origin"),
        ]);
        assert_eq!(require_same_origin(&request), Ok(()));
    }

    #[test]
    fn allows_a_request_with_no_origin_header() {
        let request = headers(&[("host", "localhost:4000")]);
        assert_eq!(require_same_origin(&request), Ok(()));
    }

    #[test]
    fn a_matching_origin_on_a_different_port_is_still_foreign() {
        let request = headers(&[
            ("origin", "http://localhost:3000"),
            ("host", "localhost:4000"),
        ]);
        assert_eq!(require_same_origin(&request), Err(Rejection::InvalidOrigin));
    }

    #[test]
    fn a_malformed_origin_is_refused() {
        let request = headers(&[("origin", "not-a-url"), ("host", "localhost:4000")]);
        assert_eq!(require_same_origin(&request), Err(Rejection::InvalidOrigin));
    }

    // --- demo session ---

    #[test]
    fn requires_a_valid_demo_session_cookie() {
        assert_eq!(
            require_demo_session(&headers(&[])),
            Err(Rejection::NoSession)
        );

        let valid = headers(&[(
            "cookie",
            "ember_demo_session=123e4567-e89b-12d3-a456-426614174000",
        )]);
        assert_eq!(
            require_demo_session(&valid),
            Ok("123e4567-e89b-12d3-a456-426614174000")
        );
    }

    #[test]
    fn a_malformed_session_cookie_is_treated_as_absent() {
        let request = headers(&[("cookie", "ember_demo_session=not-a-uuid")]);
        assert_eq!(require_demo_session(&request), Err(Rejection::NoSession));
    }

    #[test]
    fn the_session_cookie_is_found_among_others() {
        let request = headers(&[(
            "cookie",
            "theme=dark; ember_demo_session=123e4567-e89b-12d3-a456-426614174000; other=1",
        )]);
        assert!(demo_session(&request).is_some());
    }

    // --- rate limiting ---

    #[test]
    fn limits_repeated_requests_for_the_same_session() {
        let limiter = RateLimiter::new();
        let request = headers(&[
            (
                "cookie",
                "ember_demo_session=123e4567-e89b-12d3-a456-426614174001",
            ),
            ("x-forwarded-for", "203.0.113.8"),
        ]);
        let window = Duration::from_secs(60);

        assert_eq!(limiter.check(&request, "test", 2, window), Ok(()));
        assert_eq!(limiter.check(&request, "test", 2, window), Ok(()));
        assert!(matches!(
            limiter.check(&request, "test", 2, window),
            Err(Rejection::RateLimited { .. })
        ));
    }

    #[test]
    fn limits_each_identity_separately() {
        // The original test reused a single request, so a limiter keyed only on
        // `scope` — one visitor locking out everyone — would still have passed.
        let limiter = RateLimiter::new();
        let window = Duration::from_secs(60);

        let first = headers(&[
            (
                "cookie",
                "ember_demo_session=123e4567-e89b-12d3-a456-426614174001",
            ),
            ("x-forwarded-for", "203.0.113.8"),
        ]);
        let second = headers(&[
            (
                "cookie",
                "ember_demo_session=123e4567-e89b-12d3-a456-426614174002",
            ),
            ("x-forwarded-for", "203.0.113.9"),
        ]);

        // Exhaust the first caller's quota entirely.
        assert_eq!(limiter.check(&first, "test", 1, window), Ok(()));
        assert!(limiter.check(&first, "test", 1, window).is_err());

        // A different caller must be unaffected.
        assert_eq!(
            limiter.check(&second, "test", 1, window),
            Ok(()),
            "one caller exhausting a quota must not lock out everyone else"
        );
    }

    #[test]
    fn separate_scopes_do_not_share_a_bucket() {
        let limiter = RateLimiter::new();
        let request = headers(&[("x-forwarded-for", "203.0.113.8")]);
        let window = Duration::from_secs(60);

        assert_eq!(limiter.check(&request, "scope-a", 1, window), Ok(()));
        assert!(limiter.check(&request, "scope-a", 1, window).is_err());
        assert_eq!(limiter.check(&request, "scope-b", 1, window), Ok(()));
    }

    #[test]
    fn the_window_expires() {
        let limiter = RateLimiter::new();
        let request = headers(&[("x-forwarded-for", "203.0.113.8")]);

        // A zero-length window means every entry is already stale.
        assert_eq!(limiter.check(&request, "test", 1, Duration::ZERO), Ok(()));
        assert_eq!(limiter.check(&request, "test", 1, Duration::ZERO), Ok(()));
    }

    #[test]
    fn a_rate_limited_rejection_carries_retry_after() {
        let limiter = RateLimiter::new();
        let request = headers(&[("x-forwarded-for", "203.0.113.8")]);
        let window = Duration::from_secs(60);

        limiter.check(&request, "test", 1, window).unwrap();
        match limiter.check(&request, "test", 1, window) {
            Err(Rejection::RateLimited { retry_after_secs }) => {
                assert!((1..=60).contains(&retry_after_secs), "{retry_after_secs}");
            }
            other => panic!("expected a rate-limit rejection, got {other:?}"),
        }
    }

    // --- session cookie ---

    #[test]
    fn creates_a_strict_http_only_session_cookie() {
        let cookie = session_cookie("123e4567-e89b-12d3-a456-426614174000", false);
        assert!(cookie.contains("HttpOnly"));
        assert!(cookie.contains("SameSite=Strict"));
        assert!(cookie.contains("Max-Age=3600"));
        assert!(
            !cookie.contains("Secure"),
            "plain http must not set Secure, or the browser drops the cookie"
        );
    }

    #[test]
    fn marks_the_cookie_secure_when_served_over_https() {
        // Unreachable in the original test: `Secure` was gated on NODE_ENV, and
        // vitest always ran with NODE_ENV=test.
        let cookie = session_cookie("123e4567-e89b-12d3-a456-426614174000", true);
        assert!(cookie.contains("; Secure"));
        assert!(cookie.contains("HttpOnly"));
    }
}
