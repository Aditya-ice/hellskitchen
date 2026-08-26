//! Menu-bar ticket counter.
//!
//! Subscribes to the same broadcast channel that feeds the SSE stream, so the
//! title tracks the floor without polling and without a second source of truth.

use ember_core::{OrderStatus, PosState};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager};

use crate::{ui, Desktop};

pub fn install(app: &AppHandle) -> tauri::Result<()> {
    let tray = TrayIconBuilder::with_id("ember")
        .icon(app.default_window_icon().cloned().ok_or_else(|| {
            tauri::Error::AssetNotFound("the bundled application icon".into())
        })?)
        .icon_as_template(true)
        .tooltip("Ember POS")
        .on_tray_icon_event(|tray, event| {
            // Clicking the menu-bar item brings the POS forward.
            if let tauri::tray::TrayIconEvent::Click { .. } = event {
                ui::focus_main_window(tray.app_handle());
            }
        })
        .build(app)?;

    let Some(desktop) = app.try_state::<Desktop>() else {
        return Ok(());
    };
    tray.set_title(Some(summarise(
        &desktop.server.store.revision().map(|r| r.state).unwrap_or(
            PosState {
                tables: vec![],
                guests: vec![],
                orders: vec![],
                activity: vec![],
            },
        ),
    )))?;

    let mut updates = desktop.server.subscribe();
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        while let Ok(revision) = updates.recv().await {
            let title = summarise(&revision.state);
            if let Some(tray) = handle.tray_by_id("ember") {
                let _ = tray.set_title(Some(title));
            }
        }
    });

    Ok(())
}

/// "🔥 4 tickets · oldest 8m", or nothing at all when the pass is clear.
///
/// An empty title collapses the menu-bar item to just the icon, which is what
/// you want when there is nothing to report.
pub fn summarise(state: &PosState) -> String {
    let open: Vec<_> = state
        .orders
        .iter()
        .filter(|order| order.status == OrderStatus::Sent)
        .collect();

    if open.is_empty() {
        return String::new();
    }

    let oldest = open
        .iter()
        .filter_map(|order| order.sent_at.as_deref())
        .filter_map(|at| chrono::DateTime::parse_from_rfc3339(at).ok())
        .map(|at| (chrono::Utc::now() - at.with_timezone(&chrono::Utc)).num_minutes())
        .max()
        .unwrap_or(0)
        .max(0);

    format!(
        "🔥 {} ticket{} · oldest {oldest}m",
        open.len(),
        if open.len() == 1 { "" } else { "s" }
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use ember_core::{seed, Action, ActionKind};

    fn apply(state: PosState, kind: ActionKind, id: &str) -> PosState {
        ember_core::reduce(
            &state,
            &Action {
                id: id.into(),
                at: chrono::Utc::now().to_rfc3339(),
                kind,
            },
        )
        .expect("action applied")
    }

    #[test]
    fn a_clear_pass_shows_nothing() {
        assert_eq!(summarise(&seed::initial_state()), "");
    }

    #[test]
    fn one_ticket_is_singular() {
        let state = apply(
            seed::initial_state(),
            ActionKind::SeatGuest {
                guest_id: "guest-maya".into(),
                table_id: "t2".into(),
            },
            "a1",
        );
        let state = apply(
            state,
            ActionKind::AddOrderItem {
                guest_id: "guest-maya".into(),
                menu_item_id: "beet-salad".into(),
            },
            "a2",
        );
        let state = apply(
            state,
            ActionKind::SendOrder {
                guest_id: "guest-maya".into(),
            },
            "a3",
        );

        let title = summarise(&state);
        assert!(title.starts_with("🔥 1 ticket ·"), "{title}");
        assert!(!title.contains("tickets"), "{title}");
    }

    #[test]
    fn a_draft_order_is_not_a_ticket() {
        // Seating opens a draft order. Nothing has been fired, so the pass is
        // still clear.
        let state = apply(
            seed::initial_state(),
            ActionKind::SeatGuest {
                guest_id: "guest-maya".into(),
                table_id: "t2".into(),
            },
            "a1",
        );
        assert!(!state.orders.is_empty());
        assert_eq!(summarise(&state), "");
    }
}
