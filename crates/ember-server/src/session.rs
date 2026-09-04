//! Staff sessions over HTTP.
//!
//! This replaces the "demo session" that came before it, which was a cookie the
//! server minted on request and then only ever checked the *shape* of — 36
//! characters of hex and dashes. It was never stored, never validated against
//! anything, and never revoked; it was a rate-limit bucket key wearing the word
//! "session". Anything that could reach the port could forge one, and the whole
//! mutation surface was unguarded regardless.
//!
//! What is here now is an actual credential: a random token the server issues
//! only after verifying a PIN, stored as a hash, resolvable to a staff member
//! and a terminal, and expiring when a screen is left alone.

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use chrono::Utc;
use ember_store::auth::{Session, SESSION_IDLE_MINUTES};

use crate::Shared;

pub const SESSION_COOKIE: &str = "ember_session";

/// A resolved staff session, extracted from the request cookie.
///
/// Handlers take this by argument, so a route that forgets to authenticate does
/// not compile into something that silently serves anonymous traffic — it has
/// to actively choose not to ask for it.
pub struct CurrentSession(pub Session);

/// Why a request was refused before it reached a handler.
pub struct NotAuthenticated;

impl IntoResponse for NotAuthenticated {
    fn into_response(self) -> Response {
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "error": "Sign in to use this terminal.",
                "code": "not-authenticated",
            })),
        )
            .into_response()
    }
}

impl FromRequestParts<Shared> for CurrentSession {
    type Rejection = NotAuthenticated;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Shared,
    ) -> Result<Self, Self::Rejection> {
        let token = session_cookie(&parts.headers).ok_or(NotAuthenticated)?;
        match state.store.session(token, Utc::now()) {
            Ok(Some(session)) => Ok(CurrentSession(session)),
            // A store failure is deliberately indistinguishable from a bad
            // token here: the alternative leaks whether a token exists.
            _ => Err(NotAuthenticated),
        }
    }
}

/// The socket address the request arrived on, when there is one.
///
/// `ConnectInfo` itself is not an optional extractor, and a router driven
/// in-process by the tests has no peer at all — so this reads the extension
/// directly and answers `None` rather than rejecting the request.
pub struct PeerAddr(pub Option<std::net::SocketAddr>);

impl<S: Send + Sync> FromRequestParts<S> for PeerAddr {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(PeerAddr(
            parts
                .extensions
                .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
                .map(|info| info.0),
        ))
    }
}

/// Reads one cookie out of the `Cookie` header.
pub fn cookie<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get("cookie")
        .and_then(|value| value.to_str().ok())?
        .split(';')
        .find_map(|part| {
            let (key, value) = part.split_once('=')?;
            (key.trim() == name).then_some(value.trim())
        })
}

pub fn session_cookie(headers: &HeaderMap) -> Option<&str> {
    cookie(headers, SESSION_COOKIE).filter(|value| !value.is_empty())
}

/// `Set-Cookie` for a freshly issued session.
///
/// `HttpOnly` so script cannot read it, `SameSite=Strict` so another site
/// cannot ride it, and `Max-Age` matching the server-side idle expiry so the
/// browser forgets it at roughly the moment the server does.
pub fn issue(token: &str, secure: bool) -> String {
    let mut cookie = format!(
        "{SESSION_COOKIE}={token}; Path=/; HttpOnly; SameSite=Strict; Max-Age={}",
        SESSION_IDLE_MINUTES * 60
    );
    if secure {
        cookie.push_str("; Secure");
    }
    cookie
}

/// `Set-Cookie` that clears the session.
pub fn clear(secure: bool) -> String {
    let mut cookie =
        format!("{SESSION_COOKIE}=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0").to_string();
    if secure {
        cookie.push_str("; Secure");
    }
    cookie
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_session_cookie_is_http_only_and_same_site_strict() {
        let cookie = issue("a-token", false);
        assert!(
            cookie.contains("HttpOnly"),
            "script must not read the session"
        );
        assert!(cookie.contains("SameSite=Strict"));
        assert!(
            !cookie.contains("Secure"),
            "plain http must not set Secure, or the browser drops the cookie"
        );
    }

    #[test]
    fn it_is_marked_secure_when_served_over_https() {
        let cookie = issue("a-token", true);
        assert!(cookie.contains("; Secure"));
        assert!(cookie.contains("HttpOnly"));
    }

    #[test]
    fn the_cookie_expires_with_the_server_side_session() {
        // If the browser held it longer than the server honours it, staff would
        // be sent to a screen that looks signed in and refuses every action.
        let cookie = issue("a-token", false);
        assert!(cookie.contains(&format!("Max-Age={}", SESSION_IDLE_MINUTES * 60)));
    }

    #[test]
    fn clearing_expires_the_cookie_immediately() {
        assert!(clear(false).contains("Max-Age=0"));
    }

    #[test]
    fn the_session_is_read_from_among_other_cookies() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "cookie",
            "theme=dark; ember_session=the-token; other=1"
                .parse()
                .unwrap(),
        );
        assert_eq!(session_cookie(&headers), Some("the-token"));
    }

    #[test]
    fn an_absent_or_empty_session_is_none() {
        assert_eq!(session_cookie(&HeaderMap::new()), None);

        let mut headers = HeaderMap::new();
        headers.insert("cookie", "ember_session=".parse().unwrap());
        assert_eq!(session_cookie(&headers), None);
    }
}
