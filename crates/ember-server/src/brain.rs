//! Proxy to the optional Python service.
//!
//! The POS is fully functional without it, so every failure here — not
//! configured, unreachable, erroring — degrades to a plain answer the staff can
//! read rather than an error that interrupts anything. `ember-server` is the
//! only thing that talks to the brain, which keeps the UI on one origin and the
//! model credentials off the browser entirely.

use std::time::Duration;

use serde::{Deserialize, Serialize};

/// This type sits on a language boundary and is renamed in each direction.
/// The Python service speaks snake_case, the browser expects camelCase, and
/// this proxy is where the two meet. A single `rename_all` would silently
/// drop `tools_used` on the way in and default it to empty.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase", deserialize = "snake_case"))]
pub struct AgentAnswer {
    pub answer: String,
    #[serde(default)]
    pub tools_used: Vec<String>,
    #[serde(default)]
    pub model: Option<String>,
    /// False when the service is running but has no model credentials.
    #[serde(default)]
    pub configured: bool,
}

impl AgentAnswer {
    fn unavailable(answer: &str) -> Self {
        Self {
            answer: answer.into(),
            tools_used: vec![],
            model: None,
            configured: false,
        }
    }

    /// The brain is not running at all.
    pub fn not_running() -> Self {
        Self::unavailable(
            "The floor agent is not running. Start it with `npm run brain`, or leave it off — \
             the POS does not need it.",
        )
    }

    /// It is running but could not be reached or failed mid-answer.
    pub fn unreachable() -> Self {
        Self::unavailable("The floor agent is unavailable right now. The POS is unaffected.")
    }
}

/// Asks the brain a question.
///
/// Never returns an error: the caller turns this straight into a 200 so that a
/// missing optional service cannot look like a broken POS.
pub async fn ask(client: &reqwest::Client, base: &str, question: &str) -> AgentAnswer {
    let request = client
        .post(format!("{}/ask", base.trim_end_matches('/')))
        .json(&serde_json::json!({ "question": question }))
        // Agent turns think and call tools; they are slower than a page load.
        .timeout(Duration::from_secs(90))
        .send()
        .await;

    match request {
        Ok(response) if response.status().is_success() => match response.json().await {
            Ok(answer) => answer,
            Err(error) => {
                eprintln!("could not decode the floor agent response: {error}");
                AgentAnswer::unreachable()
            }
        },
        Ok(response) => {
            eprintln!("floor agent returned HTTP {}", response.status());
            AgentAnswer::unreachable()
        }
        Err(error) => {
            eprintln!("could not reach the floor agent: {error}");
            AgentAnswer::unreachable()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_absent_service_says_how_to_start_it_and_that_it_is_optional() {
        let answer = AgentAnswer::not_running();
        assert!(!answer.configured);
        assert!(answer.answer.contains("npm run brain"), "{}", answer.answer);
        assert!(answer.answer.contains("does not need it"), "{}", answer.answer);
    }

    #[test]
    fn an_unreachable_service_reassures_about_the_pos() {
        let answer = AgentAnswer::unreachable();
        assert!(answer.answer.contains("POS is unaffected"));
    }

    #[test]
    fn a_brain_response_decodes() {
        let answer: AgentAnswer = serde_json::from_value(serde_json::json!({
            "answer": "Priya has waited longest.",
            "tools_used": ["query_floor"],
            "model": "claude-opus-5",
            "configured": true
        }))
        .unwrap();

        assert_eq!(answer.answer, "Priya has waited longest.");
        assert_eq!(answer.tools_used, ["query_floor"]);
        assert!(answer.configured);
    }

    #[test]
    fn the_browser_sees_camel_case_even_though_python_sent_snake_case() {
        let answer: AgentAnswer = serde_json::from_value(serde_json::json!({
            "answer": "ok",
            "tools_used": ["query_stock"],
            "configured": true
        }))
        .unwrap();

        let encoded = serde_json::to_value(&answer).unwrap();
        assert_eq!(encoded["toolsUsed"][0], "query_stock");
        assert!(
            encoded.get("tools_used").is_none(),
            "the browser shape must not leak Python's naming"
        );
    }

    #[test]
    fn a_minimal_response_still_decodes() {
        // Missing optional fields must not turn a real answer into a failure.
        let answer: AgentAnswer =
            serde_json::from_value(serde_json::json!({ "answer": "Two tables free." })).unwrap();
        assert_eq!(answer.answer, "Two tables free.");
        assert!(answer.tools_used.is_empty());
    }
}
