//! Server configuration, read from the environment.

use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Config {
    pub host: String,
    pub port: u16,
    /// `None` runs entirely in memory — used by tests and `--ephemeral`.
    pub database: Option<PathBuf>,
    /// Directory holding the exported Next.js bundle (`out/`).
    pub static_dir: Option<PathBuf>,
    pub elevenlabs_key: Option<String>,
    pub tavily_key: Option<String>,
    /// Upstream base URLs. Overridable so the proxy code can be exercised
    /// against a local stub without real credentials, and so a staging endpoint
    /// can be substituted without a rebuild.
    pub elevenlabs_base: String,
    pub tavily_base: String,
    /// Marks the session cookie `Secure`. Off for plain-http localhost, since a
    /// browser silently drops a `Secure` cookie sent over http.
    pub secure_cookies: bool,
    /// Whether to believe `X-Forwarded-For`. Only true behind a proxy that
    /// actually sets it; otherwise any client can choose its own rate-limit
    /// identity just by sending the header.
    pub trust_forwarded_for: bool,
    /// Base URL of the optional Python service. The POS works without it.
    pub brain_url: Option<String>,
}

fn env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

impl Default for Config {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".into(),
            port: 4000,
            database: None,
            static_dir: None,
            elevenlabs_key: None,
            tavily_key: None,
            elevenlabs_base: "https://api.elevenlabs.io".into(),
            tavily_base: "https://api.tavily.com".into(),
            secure_cookies: false,
            trust_forwarded_for: false,
            brain_url: None,
        }
    }
}

impl Config {
    pub fn from_env() -> Self {
        let defaults = Config::default();
        Config {
            host: env("EMBER_HOST").unwrap_or(defaults.host),
            port: env("EMBER_PORT")
                .and_then(|value| value.parse().ok())
                .unwrap_or(defaults.port),
            database: env("EMBER_DB").map(PathBuf::from),
            static_dir: env("EMBER_STATIC_DIR").map(PathBuf::from),
            elevenlabs_key: env("ELEVENLABS_API_KEY"),
            tavily_key: env("TAVILY_API_KEY"),
            elevenlabs_base: env("EMBER_ELEVENLABS_BASE").unwrap_or(defaults.elevenlabs_base),
            tavily_base: env("EMBER_TAVILY_BASE").unwrap_or(defaults.tavily_base),
            secure_cookies: env("EMBER_SECURE_COOKIES")
                .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
                .unwrap_or(defaults.secure_cookies),
            trust_forwarded_for: env("EMBER_TRUST_PROXY")
                .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
                .unwrap_or(defaults.trust_forwarded_for),
            brain_url: env("EMBER_BRAIN_URL"),
        }
    }
}
