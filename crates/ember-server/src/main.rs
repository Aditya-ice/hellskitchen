//! Standalone Ember POS server.
//!
//! The desktop app embeds the same library; this binary is what you run when
//! you want the floor reachable from a browser or from a phone on the LAN.

use ember_server::{AppState, Config};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::from_env();

    if config.database.is_none() {
        eprintln!("warning: EMBER_DB is not set — this service will not survive a restart.");
    }
    if config.static_dir.is_none() {
        eprintln!(
            "note: EMBER_STATIC_DIR is not set — serving the API only. Run `npm run build` and \
             point it at ./out to serve the UI too."
        );
    }

    let state = AppState::new(config)?;
    ember_server::serve(state).await?;
    Ok(())
}
