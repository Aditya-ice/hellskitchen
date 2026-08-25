//! Dumps the full engine output for every seeded guest as JSON.
//!
//! Paired with `scripts/dump-ts-engine.test.ts`, this is the parity harness for
//! the TypeScript → Rust port: both sides dump the same structure and the two
//! files must be byte-identical after JSON normalisation.
//!
//! Run with: `cargo run -p ember-core --example dump`

use ember_core::{engine, seed};

fn main() {
    let tables = seed::tables();
    let menu_items = seed::menu_items();
    let ingredients = seed::ingredients();

    let dump: Vec<serde_json::Value> = seed::guests()
        .iter()
        .map(|guest| {
            serde_json::json!({
                "guestId": guest.id,
                "estimateWait": engine::estimate_wait(guest, &tables),
                "tables": engine::recommend_tables(guest, &tables),
                "dishes": engine::recommend_dishes(guest, &menu_items, &ingredients),
            })
        })
        .collect();

    println!("{}", serde_json::to_string_pretty(&dump).unwrap());
}
