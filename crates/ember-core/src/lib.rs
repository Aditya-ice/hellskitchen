//! Ember POS core: domain model, decision engine, and state reducer.
//!
//! Pure logic only — no I/O, no database, no HTTP. Persistence lives in
//! `ember-store` and transport in `ember-server`, so this crate can be reused
//! unchanged by the desktop shell, a future SwiftUI client, or a WASM build
//! running the same rules in the browser.

pub mod domain;
pub mod engine;
pub mod reducer;
pub mod seed;

pub use domain::*;
pub use engine::{
    can_seat_guest_at_table, estimate_wait, order_total, recommend_dishes, recommend_tables,
};
pub use reducer::{reduce, Action, ActionKind};
