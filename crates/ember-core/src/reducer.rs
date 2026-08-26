//! State transitions. Ported from `reducePosState` in `components/pos-provider.tsx`.
//!
//! Two defects from the TypeScript original are fixed here, both flagged by the
//! `/code-review` pass on commit 4127d36:
//!
//! 1. `SeatGuest` used to overwrite guest status with `Seated` unconditionally,
//!    so moving a party whose order was already in the kitchen silently
//!    downgraded them from `Ordered` back to `Seated`.
//! 2. The three order mutations disagreed about how to guard on order status —
//!    two denied `"sent"`, one allowed `"draft"`. Equivalent only for a
//!    two-value enum. They now share one `is_open` allow-list.

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::domain::*;
use crate::engine::can_seat_guest_at_table;
use crate::seed;

/// An action plus the identity and timestamp assigned when it was created.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Action {
    pub id: String,
    pub at: String,
    #[serde(flatten)]
    #[ts(flatten)]
    pub kind: ActionKind,
}

/// Mirrors the `SharedAction` union in `components/pos-provider.tsx`.
///
/// `AddWalkIn` carries a whole `GuestProfile`, which makes the enum much larger
/// than its other variants. Boxing it would hide the one-to-one mapping to the
/// wire format for no practical gain: these are deserialized from HTTP requests
/// at human pace, never in a hot loop.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "kebab-case", rename_all_fields = "camelCase")]
#[ts(export)]
pub enum ActionKind {
    CheckIn {
        guest_id: String,
    },
    AddWalkIn {
        guest: GuestProfile,
    },
    UpdateGuestNotes {
        guest_id: String,
        notes: String,
    },
    SeatGuest {
        guest_id: String,
        table_id: String,
    },
    AddOrderItem {
        guest_id: String,
        menu_item_id: String,
    },
    RemoveOrderItem {
        guest_id: String,
        menu_item_id: String,
    },
    UpdateOrderNotes {
        guest_id: String,
        notes: String,
    },
    SendOrder {
        guest_id: String,
    },
    Reset,
}

impl ActionKind {
    /// Short label used for the `kind` column of the action log.
    pub fn label(&self) -> &'static str {
        match self {
            ActionKind::CheckIn { .. } => "check-in",
            ActionKind::AddWalkIn { .. } => "add-walk-in",
            ActionKind::UpdateGuestNotes { .. } => "update-guest-notes",
            ActionKind::SeatGuest { .. } => "seat-guest",
            ActionKind::AddOrderItem { .. } => "add-order-item",
            ActionKind::RemoveOrderItem { .. } => "remove-order-item",
            ActionKind::UpdateOrderNotes { .. } => "update-order-notes",
            ActionKind::SendOrder { .. } => "send-order",
            ActionKind::Reset => "reset",
        }
    }
}

/// An order accepts edits only while it is still a draft.
fn is_open(order: &Order) -> bool {
    match order.status {
        OrderStatus::Draft => true,
        OrderStatus::Sent => false,
    }
}

fn activity(action: &Action, label: &str, detail: String) -> ActivityEvent {
    ActivityEvent {
        id: format!("{}-activity", action.id),
        at: action.at.clone(),
        action: label.into(),
        detail,
    }
}

/// `new Date(at).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" })`
fn clock_time(at: &str) -> String {
    match DateTime::parse_from_rfc3339(at) {
        Ok(parsed) => parsed
            .with_timezone(&Local)
            .format("%-I:%M %p")
            .to_string(),
        Err(_) => at.to_string(),
    }
}

/// Applies `action` to `state`.
///
/// Returns `None` when the action was rejected by a guard or would not change
/// anything, so callers can skip persisting and broadcasting a no-op.
pub fn reduce(state: &PosState, action: &Action) -> Option<PosState> {
    let next = apply(state, action)?;
    if next == *state {
        None
    } else {
        Some(next)
    }
}

fn apply(state: &PosState, action: &Action) -> Option<PosState> {
    let mut next = state.clone();

    match &action.kind {
        ActionKind::CheckIn { guest_id } => {
            let guest = state.guest(guest_id)?;
            if guest.status != GuestStatus::Expected {
                return None;
            }
            let name = guest.name.clone();
            for guest in next.guests.iter_mut().filter(|item| item.id == *guest_id) {
                guest.status = GuestStatus::Waiting;
                guest.arrival_time = Some(clock_time(&action.at));
            }
            next.activity.insert(
                0,
                activity(
                    action,
                    "Guest checked in",
                    format!("{name} joined the arrivals queue"),
                ),
            );
        }

        ActionKind::AddWalkIn { guest } => {
            if state.guests.iter().any(|item| item.id == guest.id) {
                return None;
            }
            let detail = format!("{}, party of {}", guest.name, guest.party_size);
            next.guests.push(guest.clone());
            next.activity
                .insert(0, activity(action, "Walk-in added", detail));
        }

        ActionKind::UpdateGuestNotes { guest_id, notes } => {
            for guest in next.guests.iter_mut().filter(|item| item.id == *guest_id) {
                guest.notes = notes.clone();
            }
        }

        ActionKind::SeatGuest { guest_id, table_id } => {
            let guest = state.guest(guest_id)?;
            let target = state.table(table_id)?;
            let current_table_id = state.table_seating(guest_id).map(|table| table.id.clone());

            if current_table_id.as_deref() == Some(target.id.as_str())
                || !can_seat_guest_at_table(guest, target)
            {
                return None;
            }

            let guest_name = guest.name.clone();
            let target_label = target.label.clone();
            let moved = current_table_id.is_some();

            for table in next.tables.iter_mut() {
                if Some(table.id.as_str()) == current_table_id.as_deref() {
                    table.status = TableStatus::Available;
                    table.seated_guest_id = None;
                    table.seated_at = None;
                } else if table.id == *table_id {
                    table.status = TableStatus::Occupied;
                    table.seated_guest_id = Some(guest_id.clone());
                    table.seated_at = Some(action.at.clone());
                }
            }

            for guest in next.guests.iter_mut().filter(|item| item.id == *guest_id) {
                // Moving a party must not walk back an order that is already in
                // the kitchen — only promote guests who have not ordered yet.
                if guest.status != GuestStatus::Ordered {
                    guest.status = GuestStatus::Seated;
                }
            }

            match next
                .orders
                .iter_mut()
                .find(|order| order.guest_id == *guest_id)
            {
                Some(order) => order.table_id = Some(table_id.clone()),
                None => next.orders.push(Order {
                    id: format!("order-{}", action.id),
                    guest_id: guest_id.clone(),
                    table_id: Some(table_id.clone()),
                    status: OrderStatus::Draft,
                    lines: vec![],
                    guest_notes: String::new(),
                    created_at: action.at.clone(),
                    sent_at: None,
                }),
            }

            next.activity.insert(
                0,
                activity(
                    action,
                    if moved { "Party moved" } else { "Party seated" },
                    format!("{guest_name} assigned to {target_label}"),
                ),
            );
        }

        ActionKind::AddOrderItem {
            guest_id,
            menu_item_id,
        } => {
            let order = next
                .orders
                .iter_mut()
                .find(|order| order.guest_id == *guest_id && is_open(order))?;
            match order
                .lines
                .iter_mut()
                .find(|line| line.menu_item_id == *menu_item_id)
            {
                Some(line) => line.quantity += 1,
                None => order.lines.push(OrderLine {
                    menu_item_id: menu_item_id.clone(),
                    quantity: 1,
                    notes: String::new(),
                }),
            }
        }

        ActionKind::RemoveOrderItem {
            guest_id,
            menu_item_id,
        } => {
            let order = next
                .orders
                .iter_mut()
                .find(|order| order.guest_id == *guest_id && is_open(order))?;
            for line in order
                .lines
                .iter_mut()
                .filter(|line| line.menu_item_id == *menu_item_id)
            {
                line.quantity = line.quantity.saturating_sub(1);
            }
            order.lines.retain(|line| line.quantity > 0);
        }

        ActionKind::UpdateOrderNotes { guest_id, notes } => {
            let order = next
                .orders
                .iter_mut()
                .find(|order| order.guest_id == *guest_id && is_open(order))?;
            order.guest_notes = notes.clone();
        }

        ActionKind::SendOrder { guest_id } => {
            let guest = state.guest(guest_id)?;
            let guest_name = guest.name.clone();

            let order = next
                .orders
                .iter_mut()
                .find(|order| order.guest_id == *guest_id)?;
            if !is_open(order) || order.lines.is_empty() {
                return None;
            }
            order.status = OrderStatus::Sent;
            order.sent_at = Some(action.at.clone());

            for guest in next.guests.iter_mut().filter(|item| item.id == *guest_id) {
                guest.status = GuestStatus::Ordered;
            }

            next.activity.insert(
                0,
                activity(
                    action,
                    "Order sent",
                    format!("{guest_name} order sent to kitchen"),
                ),
            );
        }

        ActionKind::Reset => {
            next = seed::initial_state();
        }
    }

    Some(next)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn action(kind: ActionKind) -> Action {
        Action {
            id: "test-action".into(),
            at: "2026-08-13T10:00:00.000Z".into(),
            kind,
        }
    }

    fn state() -> PosState {
        seed::initial_state()
    }

    fn seat(guest_id: &str, table_id: &str) -> Action {
        action(ActionKind::SeatGuest {
            guest_id: guest_id.into(),
            table_id: table_id.into(),
        })
    }

    // --- ported from components/pos-provider.test.ts ---

    #[test]
    fn rejects_invalid_seating_and_accepts_a_compatible_waiting_guest() {
        let initial = state();

        // Jordan is still "expected" — not checked in, so not seatable.
        assert!(reduce(&initial, &seat("guest-jordan", "t7")).is_none());
        // t3 is already occupied by Noah.
        assert!(reduce(&initial, &seat("guest-maya", "t3")).is_none());

        let valid = reduce(&initial, &seat("guest-maya", "t2")).expect("seating accepted");
        let table = valid.table("t2").unwrap();
        assert_eq!(table.status, TableStatus::Occupied);
        assert_eq!(table.seated_guest_id.as_deref(), Some("guest-maya"));
    }

    #[test]
    fn checks_in_expected_guests_only_once() {
        let initial = state();
        let checked_in = reduce(&initial, &action(ActionKind::CheckIn { guest_id: "guest-jordan".into() }))
            .expect("check-in accepted");

        assert_eq!(
            checked_in.guest("guest-jordan").unwrap().status,
            GuestStatus::Waiting
        );
        assert_eq!(checked_in.activity.len(), 1);

        let repeated = reduce(
            &checked_in,
            &action(ActionKind::CheckIn {
                guest_id: "guest-jordan".into(),
            }),
        );
        assert!(repeated.is_none(), "a second check-in must be a no-op");
    }

    #[test]
    fn keeps_sent_orders_immutable() {
        // Reach "sent" through the real transition rather than fabricating it,
        // so the guards on the transition itself are exercised.
        let seated = reduce(&state(), &seat("guest-maya", "t2")).unwrap();
        let with_item = reduce(
            &seated,
            &action(ActionKind::AddOrderItem {
                guest_id: "guest-maya".into(),
                menu_item_id: "beet-salad".into(),
            }),
        )
        .unwrap();
        let sent = reduce(
            &with_item,
            &action(ActionKind::SendOrder {
                guest_id: "guest-maya".into(),
            }),
        )
        .expect("order sent");

        let order = sent.order_for_guest("guest-maya").unwrap();
        assert_eq!(order.status, OrderStatus::Sent);
        assert_eq!(
            order.sent_at.as_deref(),
            Some("2026-08-13T10:00:00.000Z"),
            "ticket age is measured from when the order was fired"
        );

        for rejected in [
            ActionKind::AddOrderItem {
                guest_id: "guest-maya".into(),
                menu_item_id: "beet-salad".into(),
            },
            ActionKind::RemoveOrderItem {
                guest_id: "guest-maya".into(),
                menu_item_id: "beet-salad".into(),
            },
            ActionKind::UpdateOrderNotes {
                guest_id: "guest-maya".into(),
                notes: "Changed".into(),
            },
        ] {
            assert!(
                reduce(&sent, &action(rejected)).is_none(),
                "a sent order must reject every edit"
            );
        }
    }

    // --- regressions the TypeScript suite could not catch ---

    #[test]
    fn moving_a_party_does_not_walk_back_an_order_already_in_the_kitchen() {
        let seated = reduce(&state(), &seat("guest-maya", "t2")).unwrap();
        let with_item = reduce(
            &seated,
            &action(ActionKind::AddOrderItem {
                guest_id: "guest-maya".into(),
                menu_item_id: "beet-salad".into(),
            }),
        )
        .unwrap();
        let sent = reduce(
            &with_item,
            &action(ActionKind::SendOrder {
                guest_id: "guest-maya".into(),
            }),
        )
        .unwrap();
        assert_eq!(
            sent.guest("guest-maya").unwrap().status,
            GuestStatus::Ordered
        );

        let moved = reduce(&sent, &seat("guest-maya", "t4")).expect("move accepted");

        assert_eq!(
            moved.guest("guest-maya").unwrap().status,
            GuestStatus::Ordered,
            "guest was downgraded to Seated while their order is in the kitchen"
        );
        assert_eq!(moved.table("t4").unwrap().seated_guest_id.as_deref(), Some("guest-maya"));
        assert_eq!(moved.table("t2").unwrap().status, TableStatus::Available);
        assert_eq!(moved.table("t2").unwrap().seated_guest_id, None);
    }

    #[test]
    fn an_empty_order_cannot_be_sent_to_the_kitchen() {
        let seated = reduce(&state(), &seat("guest-maya", "t2")).unwrap();
        assert!(seated.order_for_guest("guest-maya").unwrap().lines.is_empty());

        assert!(
            reduce(
                &seated,
                &action(ActionKind::SendOrder {
                    guest_id: "guest-maya".into()
                })
            )
            .is_none(),
            "an empty order must not reach the kitchen"
        );
    }

    #[test]
    fn an_order_cannot_be_sent_twice() {
        let seated = reduce(&state(), &seat("guest-maya", "t2")).unwrap();
        let with_item = reduce(
            &seated,
            &action(ActionKind::AddOrderItem {
                guest_id: "guest-maya".into(),
                menu_item_id: "beet-salad".into(),
            }),
        )
        .unwrap();
        let sent = reduce(
            &with_item,
            &action(ActionKind::SendOrder {
                guest_id: "guest-maya".into(),
            }),
        )
        .unwrap();

        let resent = reduce(
            &sent,
            &Action {
                id: "second-send".into(),
                at: "2026-08-13T10:05:00.000Z".into(),
                kind: ActionKind::SendOrder {
                    guest_id: "guest-maya".into(),
                },
            },
        );
        assert!(resent.is_none(), "re-sending must not duplicate the activity entry");
    }

    #[test]
    fn seating_creates_an_order_and_moving_reuses_it() {
        let seated = reduce(&state(), &seat("guest-maya", "t2")).unwrap();
        assert_eq!(
            seated.orders.iter().filter(|o| o.guest_id == "guest-maya").count(),
            1
        );

        let moved = reduce(&seated, &seat("guest-maya", "t4")).unwrap();
        let orders: Vec<_> = moved
            .orders
            .iter()
            .filter(|order| order.guest_id == "guest-maya")
            .collect();
        assert_eq!(orders.len(), 1, "moving a party must not open a second order");
        assert_eq!(orders[0].table_id.as_deref(), Some("t4"));
    }

    #[test]
    fn removing_the_last_unit_drops_the_line() {
        let seated = reduce(&state(), &seat("guest-maya", "t2")).unwrap();
        let with_item = reduce(
            &seated,
            &action(ActionKind::AddOrderItem {
                guest_id: "guest-maya".into(),
                menu_item_id: "beet-salad".into(),
            }),
        )
        .unwrap();
        let removed = reduce(
            &with_item,
            &action(ActionKind::RemoveOrderItem {
                guest_id: "guest-maya".into(),
                menu_item_id: "beet-salad".into(),
            }),
        )
        .unwrap();

        assert!(removed.order_for_guest("guest-maya").unwrap().lines.is_empty());
    }

    #[test]
    fn a_walk_in_is_added_once() {
        let walk_in = GuestProfile {
            id: "guest-walkin".into(),
            name: "Sam Reed".into(),
            party_size: 2,
            reservation_time: None,
            arrival_time: Some("7:20 PM".into()),
            status: GuestStatus::Waiting,
            allergies: vec![],
            dietary_needs: vec![],
            likes: vec![],
            dislikes: vec![],
            seating_preferences: vec![],
            visit_count: 0,
            last_visit: None,
            notes: "Walk-in guest".into(),
        };

        let added = reduce(
            &state(),
            &action(ActionKind::AddWalkIn {
                guest: walk_in.clone(),
            }),
        )
        .expect("walk-in added");
        assert_eq!(added.guests.len(), seed::guests().len() + 1);

        assert!(
            reduce(&added, &action(ActionKind::AddWalkIn { guest: walk_in })).is_none(),
            "the same walk-in must not be added twice"
        );
    }

    #[test]
    fn reset_restores_the_seeded_service() {
        let seated = reduce(&state(), &seat("guest-maya", "t2")).unwrap();
        let reset = reduce(&seated, &action(ActionKind::Reset)).expect("reset applied");
        assert_eq!(reset, seed::initial_state());
    }

    #[test]
    fn action_json_matches_the_typescript_wire_format() {
        let encoded = serde_json::to_value(seat("guest-maya", "t2")).unwrap();
        assert_eq!(
            encoded,
            serde_json::json!({
                "id": "test-action",
                "at": "2026-08-13T10:00:00.000Z",
                "type": "seat-guest",
                "guestId": "guest-maya",
                "tableId": "t2",
            })
        );

        let decoded: Action = serde_json::from_value(encoded).unwrap();
        assert_eq!(decoded, seat("guest-maya", "t2"));
    }
}
