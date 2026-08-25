//! Server-side proxies for the sponsor integrations.
//!
//! These exist so no sponsor key ever reaches the browser. Ported from
//! `app/api/elevenlabs/token/route.ts` and `app/api/tavily/search/route.ts`;
//! the request shapes were taken from the vendored JS SDKs
//! (`@elevenlabs/elevenlabs-js` → `POST /v1/single-use-token/{type}` with an
//! `xi-api-key` header, `@tavily/core` → `POST /search` with a bearer token).

use ember_core::{MenuItem, TavilyContext, TavilySource};
use serde::Deserialize;

pub const FALLBACK_DISH_CONTEXT: &str = ember_core::seed::FALLBACK_DISH_CONTEXT;

#[derive(Debug, Deserialize)]
struct SingleUseToken {
    token: String,
}

/// Mints a short-lived, single-use Scribe token for the browser.
pub async fn elevenlabs_token(
    client: &reqwest::Client,
    base: &str,
    api_key: &str,
) -> Result<String, SponsorError> {
    let response = client
        .post(format!(
            "{}/v1/single-use-token/realtime_scribe",
            base.trim_end_matches('/')
        ))
        .header("xi-api-key", api_key)
        .send()
        .await
        .map_err(|error| SponsorError::Transport(error.to_string()))?;

    if !response.status().is_success() {
        return Err(SponsorError::Upstream(response.status().as_u16()));
    }

    let body: SingleUseToken = response
        .json()
        .await
        .map_err(|error| SponsorError::Decode(error.to_string()))?;
    Ok(body.token)
}

#[derive(Debug, Deserialize)]
struct TavilyResult {
    #[serde(default)]
    title: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    content: String,
}

#[derive(Debug, Deserialize)]
struct TavilyResponse {
    #[serde(default)]
    answer: Option<String>,
    #[serde(default)]
    results: Vec<TavilyResult>,
}

/// Fetches source-linked background for a dish.
///
/// Any failure degrades to the seeded fallback text rather than surfacing an
/// error: this is colour for a server to read out, never a safety claim, and
/// the prompt tells the model to stay away from medical or allergy statements.
pub async fn tavily_context(
    client: &reqwest::Client,
    base: &str,
    api_key: &str,
    dish: &MenuItem,
) -> TavilyContext {
    let query = format!(
        "Current culinary background, seasonality, and guest-friendly description for: {}: {}. Do not provide medical or allergy safety claims.",
        dish.name, dish.description
    );

    let request = client
        .post(format!("{}/search", base.trim_end_matches('/')))
        .bearer_auth(api_key)
        .json(&serde_json::json!({
            "query": query,
            "search_depth": "basic",
            "max_results": 3,
            "include_answer": "basic",
            "topic": "general",
        }))
        .send()
        .await;

    let parsed = match request {
        Ok(response) if response.status().is_success() => response.json::<TavilyResponse>().await,
        Ok(response) => {
            eprintln!("tavily search failed: HTTP {}", response.status());
            return fallback_context();
        }
        Err(error) => {
            eprintln!("tavily search failed: {error}");
            return fallback_context();
        }
    };

    match parsed {
        Ok(body) => TavilyContext {
            answer: body.answer,
            sources: body
                .results
                .into_iter()
                .map(|result| TavilySource {
                    title: result.title,
                    url: result.url,
                    content: result.content,
                })
                .collect(),
            is_fallback: false,
        },
        Err(error) => {
            eprintln!("could not decode tavily response: {error}");
            fallback_context()
        }
    }
}

pub fn fallback_context() -> TavilyContext {
    TavilyContext {
        answer: Some(FALLBACK_DISH_CONTEXT.to_string()),
        sources: vec![],
        is_fallback: true,
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SponsorError {
    #[error("could not reach the sponsor API: {0}")]
    Transport(String),
    #[error("sponsor API returned HTTP {0}")]
    Upstream(u16),
    #[error("could not decode the sponsor response: {0}")]
    Decode(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fallback_context_is_marked_as_a_fallback() {
        let context = fallback_context();
        assert!(context.is_fallback);
        assert!(context.sources.is_empty());
        assert_eq!(context.answer.as_deref(), Some(FALLBACK_DISH_CONTEXT));
    }

    #[test]
    fn a_tavily_response_maps_onto_the_ui_shape() {
        let body: TavilyResponse = serde_json::from_value(serde_json::json!({
            "answer": "Golden beets are sweetest in autumn.",
            "results": [
                { "title": "Beets", "url": "https://example.com/beets", "content": "…" }
            ],
            "response_time": 0.4
        }))
        .unwrap();

        assert_eq!(body.results.len(), 1);
        assert_eq!(body.results[0].url, "https://example.com/beets");
        assert!(body.answer.is_some());
    }

    #[test]
    fn a_tavily_response_missing_an_answer_still_decodes() {
        let body: TavilyResponse =
            serde_json::from_value(serde_json::json!({ "results": [] })).unwrap();
        assert!(body.answer.is_none());
        assert!(body.results.is_empty());
    }

    #[test]
    fn an_elevenlabs_token_response_decodes() {
        let body: SingleUseToken =
            serde_json::from_value(serde_json::json!({ "token": "abc123" })).unwrap();
        assert_eq!(body.token, "abc123");
    }
}
