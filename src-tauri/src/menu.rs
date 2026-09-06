//! Application menu and its keyboard shortcuts.
//!
//! Every item here only moves the UI: it emits an event to the window and lets
//! the page act on it. Nothing in this menu changes the service.
//!
//! There used to be a "Reset Demo Service" item on ⌘R that wiped the floor —
//! every party, every open ticket, the larder — on every connected window at
//! once, with no confirmation, bound to the shortcut people press to reload a
//! page. Resetting a live service is not a menu-bar action, and ⌘R is the last
//! accelerator it should have had.

use tauri::menu::{MenuBuilder, MenuItemBuilder, SubmenuBuilder};
use tauri::{AppHandle, Emitter, Wry};

use crate::ui;

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
        .items(&[&application, &edit, &view])
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

    if id == "window:kitchen" {
        if let Err(error) = ui::open_kitchen_window(app) {
            eprintln!("could not open the Kitchen display: {error}");
        }
    }
}
