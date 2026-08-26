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
    /// Bumped from the pass. Addressed by order id rather than guest id: the
    /// kitchen works from tickets, not from who is sitting where.
    CompleteOrder {
        order_id: String,
    },
    /// A delivery arrived. Additive rather than "set to N", so two people
    /// booking in stock at once add up instead of overwriting each other.
    RestockIngredient {
        ingredient_id: String,
        quantity: f64,
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
            ActionKind::CompleteOrder { .. } => "complete-order",
            ActionKind::RestockIngredient { .. } => "restock-ingredient",
            ActionKind::Reset => "reset",
        }
    }
}

/// An order accepts edits only while it is still a draft.
///
/// An allow-list on purpose. The TypeScript this replaced deny-listed "sent"
/// in two places and allow-listed "draft" in a third, which was equivalent only
/// while the enum had exactly two values — adding `Completed` here is what that
/// bug was waiting for.
fn is_open(order: &Order) -> bool {
    match order.status {
        OrderStatus::Draft => true,
        OrderStatus::Sent | OrderStatus::Completed => false,
    }
}

/// How much of each ingredient a set of order lines consumes.
///
/// One unit of each listed ingredient per serving. The menu carries no
/// per-dish quantities, so this is the honest reading of the data we have —
/// enough for stock to move visibly during a service and for the low-stock and
/// unavailable rules to fire. Real recipe quantities would replace this, and
/// only this function would change.
fn consumption(lines: &[OrderLine]) -> Vec<(String, f64)> {
    let menu = seed::menu_items();
    let mut totals: Vec<(String, f64)> = Vec::new();

    for line in lines {
        let Some(item) = menu.iter().find(|item| item.id == line.menu_item_id) else {
            continue;
        };
        for ingredient_id in &item.ingredient_ids {
            let servings = line.quantity as f64;
            match totals.iter_mut().find(|(id, _)| id == ingredient_id) {
                Some((_, total)) => *total += servings,
                None => totals.push((ingredient_id.clone(), servings)),
            }
        }
    }
    totals
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
                    completed_at: None,
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

            // Stock is committed when the ticket is fired, not when it is
            // bumped: that is the point the kitchen starts cooking it.
            let consumed = consumption(&order.lines.clone());
            for (ingredient_id, quantity) in consumed {
                if let Some(ingredient) = next
                    .ingredients
                    .iter_mut()
                    .find(|item| item.id == ingredient_id)
                {
                    // Never below zero. The engine already blocks ordering a
                    // dish whose stock has run out, so this only guards
                    // against two tickets firing against the last portion.
                    ingredient.on_hand = (ingredient.on_hand - quantity).max(0.0);
                }
            }

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

        ActionKind::CompleteOrder { order_id } => {
            let order = next.orders.iter_mut().find(|order| order.id == *order_id)?;
            // Only a ticket that actually reached the kitchen can be bumped.
            if order.status != OrderStatus::Sent {
                return None;
            }
            order.status = OrderStatus::Completed;
            order.completed_at = Some(action.at.clone());

            let guest_id = order.guest_id.clone();
            let table_label = next
                .orders
                .iter()
                .find(|order| order.id == *order_id)
                .and_then(|order| order.table_id.as_deref())
                .and_then(|id| next.tables.iter().find(|table| table.id == id))
                .map(|table| table.label.clone())
                .unwrap_or_else(|| "—".into());
            let guest_name = next
                .guest(&guest_id)
                .map(|guest| guest.name.clone())
                .unwrap_or_else(|| "A party".into());

            next.activity.insert(
                0,
                activity(
                    action,
                    "Ticket completed",
                    format!("{guest_name} · {table_label} away from the pass"),
                ),
            );
        }

        ActionKind::RestockIngredient {
            ingredient_id,
            quantity,
        } => {
            // A quantity that is not a positive, finite number is a bug or a
            // malformed request, never a real delivery. Reject rather than let
            // it corrupt the larder — NaN in particular would poison every
            // later comparison and quietly make the dish unsellable forever.
            if !quantity.is_finite() || *quantity <= 0.0 {
                return None;
            }

            let ingredient = next
                .ingredients
                .iter_mut()
                .find(|item| item.id == *ingredient_id)?;
            ingredient.on_hand += quantity;

            let detail = format!(
                "{} · +{} {} ({} on hand)",
                ingredient.name,
                format_amount(*quantity),
                ingredient.unit,
                format_amount(ingredient.on_hand)
            );
            next.activity
                .insert(0, activity(action, "Stock received", detail));
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

    fn fired_ticket() -> PosState {
        let seated = reduce(&state(), &seat("guest-maya", "t2")).unwrap();
        let with_item = reduce(
            &seated,
            &action(ActionKind::AddOrderItem {
                guest_id: "guest-maya".into(),
                menu_item_id: "beet-salad".into(),
            }),
        )
        .unwrap();
        reduce(
            &with_item,
            &action(ActionKind::SendOrder {
                guest_id: "guest-maya".into(),
            }),
        )
        .unwrap()
    }

    fn complete(state: &PosState, order_id: &str, id: &str) -> Option<PosState> {
        reduce(
            state,
            &Action {
                id: id.into(),
                at: "2026-08-13T10:30:00.000Z".into(),
                kind: ActionKind::CompleteOrder {
                    order_id: order_id.into(),
                },
            },
        )
    }

    #[test]
    fn the_kitchen_can_bump_a_fired_ticket() {
        let sent = fired_ticket();
        let order_id = sent.order_for_guest("guest-maya").unwrap().id.clone();

        let bumped = complete(&sent, &order_id, "b1").expect("ticket bumped");
        let order = bumped.order_for_guest("guest-maya").unwrap();

        assert_eq!(order.status, OrderStatus::Completed);
        assert_eq!(order.completed_at.as_deref(), Some("2026-08-13T10:30:00.000Z"));
        assert_eq!(bumped.activity[0].action, "Ticket completed");
        assert!(bumped.activity[0].detail.contains("T2"), "{:?}", bumped.activity[0]);
    }

    #[test]
    fn bumping_a_ticket_leaves_the_party_where_they_are() {
        // Food leaving the pass says nothing about the table: the party is
        // still sitting there eating it.
        let sent = fired_ticket();
        let order_id = sent.order_for_guest("guest-maya").unwrap().id.clone();
        let bumped = complete(&sent, &order_id, "b1").unwrap();

        assert_eq!(
            bumped.guest("guest-maya").unwrap().status,
            GuestStatus::Ordered
        );
        let table = bumped.table("t2").unwrap();
        assert_eq!(table.status, TableStatus::Occupied);
        assert_eq!(table.seated_guest_id.as_deref(), Some("guest-maya"));
    }

    #[test]
    fn a_ticket_cannot_be_bumped_twice() {
        let sent = fired_ticket();
        let order_id = sent.order_for_guest("guest-maya").unwrap().id.clone();
        let bumped = complete(&sent, &order_id, "b1").unwrap();

        assert!(
            complete(&bumped, &order_id, "b2").is_none(),
            "a second bump must not duplicate the activity entry"
        );
    }

    #[test]
    fn a_draft_order_cannot_be_bumped() {
        // Seating opens a draft. Nothing has reached the kitchen to bump.
        let seated = reduce(&state(), &seat("guest-maya", "t2")).unwrap();
        let order_id = seated.order_for_guest("guest-maya").unwrap().id.clone();

        assert!(complete(&seated, &order_id, "b1").is_none());
    }

    #[test]
    fn bumping_an_unknown_ticket_is_rejected() {
        assert!(complete(&fired_ticket(), "order-does-not-exist", "b1").is_none());
    }

    #[test]
    fn a_completed_order_stays_immutable() {
        // The guard that matters: `is_open` allow-lists Draft, so Completed is
        // refused for the same reason Sent is.
        let sent = fired_ticket();
        let order_id = sent.order_for_guest("guest-maya").unwrap().id.clone();
        let bumped = complete(&sent, &order_id, "b1").unwrap();

        for rejected in [
            ActionKind::AddOrderItem {
                guest_id: "guest-maya".into(),
                menu_item_id: "cauliflower".into(),
            },
            ActionKind::RemoveOrderItem {
                guest_id: "guest-maya".into(),
                menu_item_id: "beet-salad".into(),
            },
            ActionKind::UpdateOrderNotes {
                guest_id: "guest-maya".into(),
                notes: "Changed".into(),
            },
            ActionKind::SendOrder {
                guest_id: "guest-maya".into(),
            },
        ] {
            assert!(
                reduce(&bumped, &action(rejected)).is_none(),
                "a completed order must reject every edit"
            );
        }
    }

    // --- stock ---

    fn on_hand(state: &PosState, id: &str) -> f64 {
        state.ingredient(id).expect("seeded ingredient").on_hand
    }

    #[test]
    fn firing_a_ticket_consumes_its_ingredients() {
        let seated = reduce(&state(), &seat("guest-maya", "t2")).unwrap();
        // Cedar Salmon uses salmon and carrot.
        let with_item = reduce(
            &seated,
            &action(ActionKind::AddOrderItem {
                guest_id: "guest-maya".into(),
                menu_item_id: "salmon-carrot".into(),
            }),
        )
        .unwrap();

        assert_eq!(on_hand(&with_item, "salmon"), 24.0, "a draft consumes nothing");
        assert_eq!(on_hand(&with_item, "carrot"), 3.0);

        let sent = reduce(
            &with_item,
            &action(ActionKind::SendOrder {
                guest_id: "guest-maya".into(),
            }),
        )
        .unwrap();

        assert_eq!(on_hand(&sent, "salmon"), 23.0);
        assert_eq!(on_hand(&sent, "carrot"), 2.0);
        // Untouched ingredients stay put.
        assert_eq!(on_hand(&sent, "beef"), 8.0);
    }

    #[test]
    fn quantity_scales_consumption() {
        let seated = reduce(&state(), &seat("guest-maya", "t2")).unwrap();
        let mut state = seated;
        for id in ["a", "b", "c"] {
            state = reduce(
                &state,
                &Action {
                    id: id.into(),
                    at: "2026-08-13T10:00:00.000Z".into(),
                    kind: ActionKind::AddOrderItem {
                        guest_id: "guest-maya".into(),
                        menu_item_id: "beet-salad".into(),
                    },
                },
            )
            .unwrap();
        }
        assert_eq!(
            state.order_for_guest("guest-maya").unwrap().lines[0].quantity,
            3
        );

        let sent = reduce(
            &state,
            &action(ActionKind::SendOrder {
                guest_id: "guest-maya".into(),
            }),
        )
        .unwrap();
        assert_eq!(on_hand(&sent, "beet"), 13.0, "16 on hand less three servings");
    }

    #[test]
    fn a_dish_becomes_ineligible_once_its_stock_runs_out() {
        // Carrots start at 3. Three carrot dishes exhaust them, and the
        // engine must then refuse anything else that needs one.
        let seated = reduce(&state(), &seat("guest-maya", "t2")).unwrap();
        let mut state = seated;
        for id in ["a", "b", "c"] {
            state = reduce(
                &state,
                &Action {
                    id: id.into(),
                    at: "2026-08-13T10:00:00.000Z".into(),
                    kind: ActionKind::AddOrderItem {
                        guest_id: "guest-maya".into(),
                        menu_item_id: "salmon-carrot".into(),
                    },
                },
            )
            .unwrap();
        }
        let sent = reduce(
            &state,
            &action(ActionKind::SendOrder {
                guest_id: "guest-maya".into(),
            }),
        )
        .unwrap();

        assert_eq!(on_hand(&sent, "carrot"), 0.0);

        let guest = sent.guest("guest-maya").unwrap();
        let dishes = crate::engine::recommend_dishes(guest, &seed::menu_items(), &sent.ingredients);
        let salmon = dishes.iter().find(|d| d.id == "salmon-carrot").unwrap();

        assert!(!salmon.eligible, "a dish with no carrots left cannot be sold");
        assert!(salmon
            .warnings
            .iter()
            .any(|w| w == "Carrots is unavailable"));
    }

    #[test]
    fn stock_never_goes_negative() {
        let seated = reduce(&state(), &seat("guest-maya", "t2")).unwrap();
        let mut state = seated;
        // Ten servings against three carrots on hand.
        for index in 0..10 {
            state = reduce(
                &state,
                &Action {
                    id: format!("a{index}"),
                    at: "2026-08-13T10:00:00.000Z".into(),
                    kind: ActionKind::AddOrderItem {
                        guest_id: "guest-maya".into(),
                        menu_item_id: "salmon-carrot".into(),
                    },
                },
            )
            .unwrap();
        }
        let sent = reduce(
            &state,
            &action(ActionKind::SendOrder {
                guest_id: "guest-maya".into(),
            }),
        )
        .unwrap();

        assert_eq!(on_hand(&sent, "carrot"), 0.0);
    }

    #[test]
    fn bumping_a_ticket_does_not_consume_stock_again() {
        let sent = fired_ticket();
        let before = on_hand(&sent, "beet");
        let order_id = sent.order_for_guest("guest-maya").unwrap().id.clone();

        let bumped = complete(&sent, &order_id, "b1").unwrap();
        assert_eq!(on_hand(&bumped, "beet"), before);
    }

    #[test]
    fn reset_restores_the_larder() {
        let sent = fired_ticket();
        assert_ne!(on_hand(&sent, "beet"), 16.0);

        let reset = reduce(&sent, &action(ActionKind::Reset)).unwrap();
        assert_eq!(on_hand(&reset, "beet"), 16.0);
    }

    // --- restocking ---

    fn restock(state: &PosState, id: &str, quantity: f64, action_id: &str) -> Option<PosState> {
        reduce(
            state,
            &Action {
                id: action_id.into(),
                at: "2026-08-13T11:00:00.000Z".into(),
                kind: ActionKind::RestockIngredient {
                    ingredient_id: id.into(),
                    quantity,
                },
            },
        )
    }

    #[test]
    fn a_delivery_adds_to_the_larder() {
        let state = state();
        assert_eq!(on_hand(&state, "carrot"), 3.0);

        let restocked = restock(&state, "carrot", 15.0, "r1").expect("delivery booked in");

        assert_eq!(on_hand(&restocked, "carrot"), 18.0);
        assert_eq!(restocked.activity[0].action, "Stock received");
        assert_eq!(
            restocked.activity[0].detail,
            "Carrots · +15 lb (18 on hand)"
        );
    }

    #[test]
    fn deliveries_accumulate() {
        // Additive, so two people booking in stock do not overwrite each other.
        let first = restock(&state(), "carrot", 5.0, "r1").unwrap();
        let second = restock(&first, "carrot", 5.0, "r2").unwrap();
        assert_eq!(on_hand(&second, "carrot"), 13.0);
    }

    #[test]
    fn restocking_brings_a_sold_out_dish_back() {
        // Exhaust carrots, then book a delivery in and confirm the engine
        // sells the dish again.
        let seated = reduce(&state(), &seat("guest-maya", "t2")).unwrap();
        let mut state = seated;
        for id in ["a", "b", "c"] {
            state = reduce(
                &state,
                &Action {
                    id: id.into(),
                    at: "2026-08-13T10:00:00.000Z".into(),
                    kind: ActionKind::AddOrderItem {
                        guest_id: "guest-maya".into(),
                        menu_item_id: "salmon-carrot".into(),
                    },
                },
            )
            .unwrap();
        }
        let sent = reduce(
            &state,
            &action(ActionKind::SendOrder {
                guest_id: "guest-maya".into(),
            }),
        )
        .unwrap();
        assert_eq!(on_hand(&sent, "carrot"), 0.0);

        let dishes_before =
            crate::engine::recommend_dishes(sent.guest("guest-maya").unwrap(), &seed::menu_items(), &sent.ingredients);
        assert!(!dishes_before.iter().find(|d| d.id == "salmon-carrot").unwrap().eligible);

        let restocked = restock(&sent, "carrot", 18.0, "r1").unwrap();
        let dishes_after = crate::engine::recommend_dishes(
            restocked.guest("guest-maya").unwrap(),
            &seed::menu_items(),
            &restocked.ingredients,
        );
        let salmon = dishes_after.iter().find(|d| d.id == "salmon-carrot").unwrap();

        assert!(salmon.eligible, "a restocked dish must be sellable again");
        assert!(
            !salmon.warnings.iter().any(|w| w.contains("unavailable")),
            "{:?}",
            salmon.warnings
        );
    }

    #[test]
    fn a_delivery_of_nothing_is_rejected() {
        for quantity in [0.0, -5.0] {
            assert!(
                restock(&state(), "carrot", quantity, "r1").is_none(),
                "quantity {quantity} should be refused"
            );
        }
    }

    #[test]
    fn a_non_finite_delivery_is_rejected() {
        // NaN would poison every later comparison: `on_hand <= 0.0` and
        // `on_hand / par <= 0.25` both go false, so the dish would look
        // available forever while holding an unusable amount.
        for quantity in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(
                restock(&state(), "carrot", quantity, "r1").is_none(),
                "quantity {quantity} should be refused"
            );
        }
        assert_eq!(on_hand(&state(), "carrot"), 3.0);
    }

    #[test]
    fn restocking_an_unknown_ingredient_is_rejected() {
        assert!(restock(&state(), "unobtainium", 5.0, "r1").is_none());
    }

    #[test]
    fn a_delivery_may_exceed_par() {
        // Par is a target, not a ceiling — a bulk delivery is legitimate.
        let restocked = restock(&state(), "beet", 100.0, "r1").unwrap();
        assert_eq!(on_hand(&restocked, "beet"), 116.0);
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
