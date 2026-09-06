//! Standalone Ember POS server.
//!
//! The desktop app embeds the same library; this binary is what you run when
//! you want the floor reachable from a browser or from a phone on the LAN.

use ember_server::{AppState, Config};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Default to info; `RUST_LOG=ember_server=debug` and friends still work.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    // Printed via Display and not propagated: returning the error from `main`
    // renders it with Debug, so "NoDatabase" is all an operator would see of a
    // message written to tell them exactly what to set.
    let config = match Config::from_env() {
        Ok(config) => config,
        Err(error) => {
            tracing::error!("{error}");
            std::process::exit(2);
        }
    };

    if config.static_dir.is_none() {
        tracing::info!(
            "EMBER_STATIC_DIR is not set — serving the API only. Run `npm run build` and point \
             it at ./out to serve the UI too."
        );
    }

    let state = AppState::new(config)?;
    ember_server::serve(state).await?;
    Ok(())
}
