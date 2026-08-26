//! Domain model for Ember POS.
//!
//! Ported from `lib/domain.ts`. The serde representation is deliberately
//! identical to the shape the web UI already sends and receives, so the
//! TypeScript client can be swapped over without touching any view code.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum DiningArea {
    Main,
    Window,
    Patio,
    Bar,
}

impl DiningArea {
    /// Lowercase label, matching the strings stored in
    /// `GuestProfile::seating_preferences`.
    pub fn as_str(self) -> &'static str {
        match self {
            DiningArea::Main => "main",
            DiningArea::Window => "window",
            DiningArea::Patio => "patio",
            DiningArea::Bar => "bar",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum TableStatus {
    Available,
    Occupied,
    Clearing,
    Reserved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum GuestStatus {
    Expected,
    Waiting,
    Seated,
    Ordered,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum OrderStatus {
    Draft,
    Sent,
    /// Bumped from the pass: the food has gone out. The party may still be
    /// seated, so this says nothing about the table.
    Completed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum MenuSection {
    Starter,
    Main,
    Side,
    Dessert,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum StaffRole {
    Host,
    Server,
    Manager,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Ingredient {
    pub id: String,
    pub name: String,
    pub aliases: Vec<String>,
    pub on_hand: f64,
    pub par: f64,
    pub unit: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MenuItem {
    pub id: String,
    pub name: String,
    pub description: String,
    pub section: MenuSection,
    pub ingredient_ids: Vec<String>,
    pub tags: Vec<String>,
    pub allergens: Vec<String>,
    // NOTE: money as f64 mirrors the TypeScript `number` this was ported from.
    // Worth moving to integer cents, but that changes the wire format and every
    // price format call in the UI, so it is deliberately left for a follow-up.
    pub price: f64,
    pub prep_minutes: f64,
    pub popularity: f64,
    pub margin_score: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Table {
    pub id: String,
    pub label: String,
    pub capacity: u32,
    pub area: DiningArea,
    pub status: TableStatus,
    pub accessible: bool,
    pub server_id: String,
    pub seated_guest_id: Option<String>,
    pub seated_at: Option<String>,
    pub estimated_available_minutes: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GuestProfile {
    pub id: String,
    pub name: String,
    pub party_size: u32,
    pub reservation_time: Option<String>,
    pub arrival_time: Option<String>,
    pub status: GuestStatus,
    pub allergies: Vec<String>,
    pub dietary_needs: Vec<String>,
    pub likes: Vec<String>,
    pub dislikes: Vec<String>,
    pub seating_preferences: Vec<String>,
    pub visit_count: u32,
    pub last_visit: Option<String>,
    pub notes: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct StaffMember {
    pub id: String,
    pub name: String,
    pub role: StaffRole,
    pub initials: String,
    // Always serialised, even when absent: ts-rs generates this as a required
    // nullable field, and skipping it would put the wire format at odds with
    // the type generated from it.
    pub section: Option<DiningArea>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct OrderLine {
    pub menu_item_id: String,
    pub quantity: u32,
    pub notes: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Order {
    pub id: String,
    pub guest_id: String,
    pub table_id: Option<String>,
    pub status: OrderStatus,
    pub lines: Vec<OrderLine>,
    pub guest_notes: String,
    pub created_at: String,
    /// When the order was fired to the kitchen. Ticket age — the number the
    /// pass actually cares about — is measured from here, not from
    /// `created_at`, which is when the party sat down.
    pub sent_at: Option<String>,
    /// When the kitchen bumped the ticket.
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ActivityEvent {
    pub id: String,
    pub at: String,
    pub action: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PosState {
    pub tables: Vec<Table>,
    pub guests: Vec<GuestProfile>,
    pub orders: Vec<Order>,
    pub activity: Vec<ActivityEvent>,
    /// Live stock. Part of the state rather than static reference data,
    /// because firing an order consumes it — which is what makes the low-stock
    /// warnings mean anything during a service.
    ///
    /// Defaulted so that a service saved before stock was tracked still loads.
    /// A full larder is the honest reading of such a snapshot: nothing was
    /// being consumed while it was written.
    #[serde(default = "crate::seed::ingredients")]
    pub ingredients: Vec<Ingredient>,
}

impl PosState {
    /// A state with nothing in it, for callers that need a placeholder before
    /// the real one has loaded. Prefer this over a struct literal so adding a
    /// field does not break every such call site.
    pub fn empty() -> Self {
        Self {
            tables: vec![],
            guests: vec![],
            orders: vec![],
            activity: vec![],
            ingredients: vec![],
        }
    }

    pub fn guest(&self, id: &str) -> Option<&GuestProfile> {
        self.guests.iter().find(|guest| guest.id == id)
    }

    pub fn table(&self, id: &str) -> Option<&Table> {
        self.tables.iter().find(|table| table.id == id)
    }

    pub fn order_for_guest(&self, guest_id: &str) -> Option<&Order> {
        self.orders.iter().find(|order| order.guest_id == guest_id)
    }

    pub fn ingredient(&self, id: &str) -> Option<&Ingredient> {
        self.ingredients.iter().find(|item| item.id == id)
    }

    pub fn table_seating(&self, guest_id: &str) -> Option<&Table> {
        self.tables
            .iter()
            .find(|table| table.seated_guest_id.as_deref() == Some(guest_id))
    }
}

/// Scored suggestion for either a table or a dish.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Recommendation {
    pub id: String,
    pub score: f64,
    pub eligible: bool,
    pub reasons: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TavilySource {
    pub title: String,
    pub url: String,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TavilyContext {
    pub answer: Option<String>,
    pub sources: Vec<TavilySource>,
    pub is_fallback: bool,
}
