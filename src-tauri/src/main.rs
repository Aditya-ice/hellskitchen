// Release builds are GUI apps: no console window, no stray stdout.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Ember POS for macOS.
//!
//! The desktop app is not a second implementation of the POS — it embeds
//! `ember-server` in-process and points a webview at it. Everything the browser
//! sees, this sees, because it is literally the same server. What the native
//! shell adds is the part a browser tab cannot do: a menu-bar ticket count, a
//! kitchen display on a second screen, and notifications that reach you when
//! the window is behind something else.

mod menu;
mod tray;
mod ui;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use ember_server::{AppState, Config};
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

/// Shared with the menu, tray and notification modules.
pub struct Desktop {
    pub server: Arc<AppState>,
    pub address: SocketAddr,
}

impl Desktop {
    pub fn url(&self, path: &str) -> String {
        format!("http://{}{path}", self.address)
    }
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let handle = app.handle().clone();

            // Bind before anything else so the window has a port to point at,
            // and so nothing else can take it in between.
            let listener = tauri::async_runtime::block_on(async {
                tokio::net::TcpListener::bind("127.0.0.1:0").await
            })?;
            let address = listener.local_addr()?;

            let static_dir = static_dir(app.handle());
            if static_dir.is_none() {
                eprintln!(
                    "Ember POS: no exported UI found. Run `npm run build` before `cargo run`."
                );
            }

            let server = AppState::new(Config {
                database: Some(database_path(app.handle())),
                static_dir,
                ..Config::from_env()
            })?;

            let desktop = Desktop {
                server: server.clone(),
                address,
            };

            tauri::async_runtime::spawn({
                let server = server.clone();
                async move {
                    if let Err(error) = ember_server::serve_on(listener, server).await {
                        eprintln!("Ember POS server stopped: {error}");
                    }
                }
            });

            let main_url = desktop.url("/pos");
            app.manage(desktop);

            WebviewWindowBuilder::new(
                app,
                "main",
                WebviewUrl::External(main_url.parse().expect("a valid loopback url")),
            )
            .title("Ember POS")
            .inner_size(1440.0, 900.0)
            .min_inner_size(1024.0, 700.0)
            .build()?;

            menu::install(&handle)?;
            tray::install(&handle)?;
            ui::watch_for_notifications(&handle);

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("Ember POS failed to start");
}

/// The service database lives in Application Support, not next to the binary,
/// so a service survives reinstalling the app.
fn database_path(app: &tauri::AppHandle) -> PathBuf {
    let directory = app
        .path()
        .app_data_dir()
        .unwrap_or_else(|_| PathBuf::from("."));
    std::fs::create_dir_all(&directory).ok();
    directory.join("ember.db")
}

/// Locates the exported UI.
///
/// In a bundled app it is a resource. `tauri.conf.json` declares it as
/// `../out`, and the bundler rewrites the leading `..` to a literal `_up_`
/// directory, so that is where it actually lands — checked first, with the
/// unprefixed path kept in case that mapping ever changes. Under `cargo run`
/// the directory is still in the working tree.
///
/// `None` means the app runs with an API but no UI; the server then serves an
/// error explaining how to build it, which is more useful than a blank window.
fn static_dir(app: &tauri::AppHandle) -> Option<PathBuf> {
    let resources = app.path().resource_dir().ok();
    let candidates = [
        resources.as_ref().map(|dir| dir.join("_up_").join("out")),
        resources.as_ref().map(|dir| dir.join("out")),
        Some(PathBuf::from("out")),
    ];

    candidates
        .into_iter()
        .flatten()
        .find(|path| path.join("index.html").is_file())
}
