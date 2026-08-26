//! Application menu and its keyboard shortcuts.
//!
//! Menu items that change the service (Reset) act on the server directly rather
//! than asking the web page to do it — the resulting revision then reaches
//! every open window, including the Kitchen display, through the same broadcast
//! any other client would get. Items that only move the UI emit an event to the
//! focused window.

use ember_core::{Action, ActionKind};
use tauri::menu::{MenuBuilder, MenuItemBuilder, SubmenuBuilder};
use tauri::{AppHandle, Emitter, Manager, Wry};

use crate::{ui, Desktop};

pub fn install(app: &AppHandle) -> tauri::Result<()> {
    let tabs = [
        ("arrivals", "Arrivals", "CmdOrCtrl+1"),
        ("floor", "Floor", "CmdOrCtrl+2"),
        ("order", "Order", "CmdOrCtrl+3"),
        ("guest", "Guest", "CmdOrCtrl+4"),
    ];

    let mut view = SubmenuBuilder::new(app, "View");
    for (id, label, accelerator) in tabs {
        view = view.item(
            &MenuItemBuilder::with_id(format!("tab:{id}"), label)
                .accelerator(accelerator)
                .build(app)?,
        );
    }
    let view = view
        .separator()
        .item(
            &MenuItemBuilder::with_id("window:kitchen", "Kitchen Display")
                .accelerator("CmdOrCtrl+K")
                .build(app)?,
        )
        .build()?;

    let service = SubmenuBuilder::new(app, "Service")
        .item(
            &MenuItemBuilder::with_id("service:reset", "Reset Demo Service")
                .accelerator("CmdOrCtrl+R")
                .build(app)?,
        )
        .build()?;

    let application = SubmenuBuilder::new(app, "Ember POS")
        .about(None)
        .separator()
        .services()
        .separator()
        .hide()
        .hide_others()
        .show_all()
        .separator()
        .quit()
        .build()?;

    let edit = SubmenuBuilder::new(app, "Edit")
        .undo()
        .redo()
        .separator()
        .cut()
        .copy()
        .paste()
        .select_all()
        .build()?;

    let menu = MenuBuilder::new(app)
        .items(&[&application, &edit, &service, &view])
        .build()?;

    app.set_menu(menu)?;
    app.on_menu_event(handle_event);
    Ok(())
}

fn handle_event(app: &AppHandle<Wry>, event: tauri::menu::MenuEvent) {
    let id = event.id().0.as_str();

    if let Some(tab) = id.strip_prefix("tab:") {
        // The page listens for this and switches tabs; harmless if the Kitchen
        // window is focused, which has no tabs.
        let _ = app.emit("ember://tab", tab);
        return;
    }

    match id {
        "window:kitchen" => {
            if let Err(error) = ui::open_kitchen_window(app) {
                eprintln!("could not open the Kitchen display: {error}");
            }
        }
        "service:reset" => reset_service(app),
        _ => {}
    }
}

/// Applies a reset the same way any client would: through the store, so it is
/// logged, versioned, and broadcast to every window.
fn reset_service(app: &AppHandle) {
    let Some(desktop) = app.try_state::<Desktop>() else {
        return;
    };
    let action = Action {
        id: uuid_v4(),
        at: chrono::Utc::now().to_rfc3339(),
        kind: ActionKind::Reset,
    };

    // Goes through the same path a browser client would, so it is logged,
    // versioned and pushed to every open window.
    if let Err(error) = desktop.server.apply(&action) {
        eprintln!("reset failed: {error}");
    }
}

fn uuid_v4() -> String {
    uuid::Uuid::new_v4().to_string()
}
