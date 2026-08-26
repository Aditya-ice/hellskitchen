//! Windows and notifications.

use std::collections::HashSet;

use ember_core::{GuestStatus, OrderStatus, PosState};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_notification::NotificationExt;

use crate::Desktop;

pub fn focus_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Opens (or focuses) the Kitchen display.
///
/// A separate window rather than a tab, because it belongs on the second screen
/// above the pass while the host keeps the POS on the terminal.
pub fn open_kitchen_window(app: &AppHandle) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window("kitchen") {
        window.show()?;
        window.set_focus()?;
        return Ok(());
    }

    let Some(desktop) = app.try_state::<Desktop>() else {
        return Ok(());
    };
    let url = desktop.url("/pos?view=kitchen");

    WebviewWindowBuilder::new(
        app,
        "kitchen",
        WebviewUrl::External(url.parse().expect("a valid loopback url")),
    )
    .title("Ember POS — Kitchen")
    .inner_size(1200.0, 820.0)
    .build()?;

    Ok(())
}

/// Watches the floor and raises a native notification for the two things worth
/// interrupting someone for.
pub fn watch_for_notifications(app: &AppHandle) {
    let Some(desktop) = app.try_state::<Desktop>() else {
        return;
    };
    let mut updates = desktop.server.subscribe();
    let mut seen = Seen::from(&desktop.server.store.revision().map(|r| r.state).unwrap_or_else(
        |_| PosState {
            tables: vec![],
            guests: vec![],
            orders: vec![],
            activity: vec![],
        },
    ));
    let handle = app.clone();

    tauri::async_runtime::spawn(async move {
        while let Ok(revision) = updates.recv().await {
            for message in seen.diff(&revision.state) {
                let _ = handle
                    .notification()
                    .builder()
                    .title(message.title)
                    .body(message.body)
                    .show();
            }
        }
    });
}

struct Message {
    title: String,
    body: String,
}

/// Tracks what has already been announced, so a notification fires on the
/// transition rather than on every revision that follows it.
struct Seen {
    sent_orders: HashSet<String>,
    seated_guests: HashSet<String>,
}

impl Seen {
    fn from(state: &PosState) -> Self {
        Self {
            sent_orders: sent_order_ids(state),
            seated_guests: seated_guest_ids(state),
        }
    }

    fn diff(&mut self, state: &PosState) -> Vec<Message> {
        let mut messages = Vec::new();

        for order in state.orders.iter().filter(|o| o.status == OrderStatus::Sent) {
            if self.sent_orders.insert(order.id.clone()) {
                let guest = state
                    .guest(&order.guest_id)
                    .map(|guest| guest.name.as_str())
                    .unwrap_or("A party");
                let table = order
                    .table_id
                    .as_deref()
                    .and_then(|id| state.table(id))
                    .map(|table| table.label.as_str())
                    .unwrap_or("—");
                messages.push(Message {
                    title: "Order sent".into(),
                    body: format!(
                        "{guest} · {table} · {} item{}",
                        order.lines.len(),
                        if order.lines.len() == 1 { "" } else { "s" }
                    ),
                });
            }
        }

        // A party with recorded allergies has just been seated. The landing
        // page promises allergies are surfaced early; this is what makes that
        // true when the window is behind something else.
        for guest in state.guests.iter().filter(|guest| {
            matches!(guest.status, GuestStatus::Seated | GuestStatus::Ordered)
        }) {
            if self.seated_guests.insert(guest.id.clone()) && !guest.allergies.is_empty() {
                let table = state
                    .table_seating(&guest.id)
                    .map(|table| table.label.as_str())
                    .unwrap_or("—");
                messages.push(Message {
                    title: format!("Allergy — {}", guest.name),
                    body: format!(
                        "{table} · {} · verify with the kitchen",
                        guest.allergies.join(", ")
                    ),
                });
            }
        }

        messages
    }
}

fn sent_order_ids(state: &PosState) -> HashSet<String> {
    state
        .orders
        .iter()
        .filter(|order| order.status == OrderStatus::Sent)
        .map(|order| order.id.clone())
        .collect()
}

fn seated_guest_ids(state: &PosState) -> HashSet<String> {
    state
        .guests
        .iter()
        .filter(|guest| matches!(guest.status, GuestStatus::Seated | GuestStatus::Ordered))
        .map(|guest| guest.id.clone())
        .collect()
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
                at: "2026-08-13T10:00:00.000Z".into(),
                kind,
            },
        )
        .expect("action applied")
    }

    fn seat(state: PosState, guest: &str, table: &str, id: &str) -> PosState {
        apply(
            state,
            ActionKind::SeatGuest {
                guest_id: guest.into(),
                table_id: table.into(),
            },
            id,
        )
    }

    #[test]
    fn seating_a_guest_with_allergies_raises_one_notification() {
        let initial = seed::initial_state();
        let mut seen = Seen::from(&initial);

        // Maya has a tree-nut allergy.
        let seated = seat(initial, "guest-maya", "t2", "a1");
        let messages = seen.diff(&seated);

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].title, "Allergy — Maya Chen");
        assert!(messages[0].body.contains("tree nuts"), "{}", messages[0].body);
        assert!(messages[0].body.contains("T2"), "{}", messages[0].body);
    }

    #[test]
    fn the_same_transition_is_not_announced_twice() {
        let initial = seed::initial_state();
        let mut seen = Seen::from(&initial);
        let seated = seat(initial, "guest-maya", "t2", "a1");

        assert_eq!(seen.diff(&seated).len(), 1);
        assert!(
            seen.diff(&seated).is_empty(),
            "a repeated revision must not re-notify"
        );

        // An unrelated change must not re-announce her either.
        let moved = seat(seated, "guest-priya", "t9", "a2");
        let messages = seen.diff(&moved);
        assert!(
            messages.iter().all(|m| !m.title.contains("Maya")),
            "Maya was announced again"
        );
    }

    #[test]
    fn a_guest_without_allergies_is_not_announced() {
        let initial = seed::initial_state();
        let mut seen = Seen::from(&initial);

        // Jordan has no allergies; check in first, since only waiting guests
        // can be seated.
        let checked_in = apply(
            initial,
            ActionKind::CheckIn {
                guest_id: "guest-jordan".into(),
            },
            "a1",
        );
        let seated = seat(checked_in, "guest-jordan", "t7", "a2");

        assert!(seen.diff(&seated).is_empty());
    }

    #[test]
    fn sending_an_order_is_announced_with_its_table() {
        let initial = seed::initial_state();
        let mut seen = Seen::from(&initial);

        let state = seat(initial, "guest-maya", "t2", "a1");
        seen.diff(&state); // drain the allergy notification

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

        let messages = seen.diff(&state);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].title, "Order sent");
        assert!(messages[0].body.contains("Maya Chen"), "{}", messages[0].body);
        assert!(messages[0].body.contains("T2"), "{}", messages[0].body);
        assert!(messages[0].body.contains("1 item"), "{}", messages[0].body);
    }

    #[test]
    fn guests_already_seated_at_startup_are_not_announced() {
        // Noah is seated in the seeded service. Restarting the app must not
        // fire a notification for a party that sat down before it launched.
        let initial = seed::initial_state();
        let mut seen = Seen::from(&initial);
        assert!(seen.diff(&initial).is_empty());
    }
}
