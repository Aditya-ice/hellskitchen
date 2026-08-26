//! Seeded demo service. Ported from `data/demo.ts`.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::domain::*;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Restaurant {
    pub name: String,
    pub short_name: String,
    pub venue: String,
    pub service_label: String,
    pub covers: u32,
}

pub const FALLBACK_DISH_CONTEXT: &str = "Seasonal preparation details are unavailable. Confirm ingredients and substitutions with the kitchen before describing them to a guest.";

pub fn restaurant() -> Restaurant {
    Restaurant {
        name: "Ember & Ash".into(),
        short_name: "E&A".into(),
        venue: "Fordham University, Lincoln Center, New York".into(),
        service_label: "Dinner service".into(),
        covers: 86,
    }
}

fn ingredient(id: &str, name: &str, aliases: &[&str], on_hand: f64, par: f64, unit: &str) -> Ingredient {
    Ingredient {
        id: id.into(),
        name: name.into(),
        aliases: aliases.iter().map(|value| (*value).into()).collect(),
        on_hand,
        par,
        unit: unit.into(),
    }
}

pub fn ingredients() -> Vec<Ingredient> {
    vec![
        ingredient("carrot", "Carrots", &["carrot", "carrots"], 3.0, 18.0, "lb"),
        ingredient("beet", "Golden beets", &["beet", "beets"], 16.0, 12.0, "lb"),
        ingredient("parsnip", "Parsnips", &["parsnip", "parsnips"], 14.0, 10.0, "lb"),
        ingredient("salmon", "Salmon", &["salmon", "fish"], 24.0, 30.0, "portions"),
        ingredient("chicken", "Chicken", &["chicken", "poultry"], 38.0, 36.0, "portions"),
        ingredient("farro", "Farro", &["farro", "grain"], 12.0, 8.0, "lb"),
        ingredient("hazelnut", "Hazelnuts", &["hazelnut", "nuts"], 5.0, 6.0, "lb"),
        ingredient("mushroom", "Mushrooms", &["mushroom", "mushrooms"], 15.0, 12.0, "lb"),
        ingredient("cauliflower", "Cauliflower", &["cauliflower"], 11.0, 9.0, "heads"),
        ingredient("beef", "Dry-aged beef", &["beef", "steak"], 8.0, 18.0, "portions"),
        ingredient("chocolate", "Dark chocolate", &["chocolate"], 10.0, 8.0, "lb"),
    ]
}

#[allow(clippy::too_many_arguments)]
fn menu_item(
    id: &str,
    name: &str,
    description: &str,
    section: MenuSection,
    ingredient_ids: &[&str],
    tags: &[&str],
    allergens: &[&str],
    price: f64,
    prep_minutes: f64,
    popularity: f64,
    margin_score: f64,
) -> MenuItem {
    MenuItem {
        id: id.into(),
        name: name.into(),
        description: description.into(),
        section,
        ingredient_ids: ingredient_ids.iter().map(|value| (*value).into()).collect(),
        tags: tags.iter().map(|value| (*value).into()).collect(),
        allergens: allergens.iter().map(|value| (*value).into()).collect(),
        price,
        prep_minutes,
        popularity,
        margin_score,
    }
}

pub fn menu_items() -> Vec<MenuItem> {
    vec![
        menu_item(
            "carrot-tartare",
            "Charred Carrot Tartare",
            "Smoked carrot, rye crisp, hazelnut, mustard seed",
            MenuSection::Starter,
            &["carrot", "hazelnut"],
            &["vegetarian"],
            &["tree nuts", "gluten"],
            18.0,
            8.0,
            91.0,
            86.0,
        ),
        menu_item(
            "beet-salad",
            "Golden Beet & Citrus",
            "Roasted golden beet, blood orange, dill, verjus",
            MenuSection::Starter,
            &["beet"],
            &["vegan", "vegetarian", "gluten-free"],
            &[],
            17.0,
            6.0,
            84.0,
            90.0,
        ),
        menu_item(
            "salmon-carrot",
            "Cedar Salmon",
            "Cedar-roasted salmon, carrot purée, charred leek",
            MenuSection::Main,
            &["salmon", "carrot"],
            &["gluten-free", "pescatarian"],
            &["fish"],
            34.0,
            16.0,
            94.0,
            72.0,
        ),
        menu_item(
            "mushroom-farro",
            "Woodland Mushroom Farro",
            "Roasted mushrooms, farro, black garlic, pecorino",
            MenuSection::Main,
            &["mushroom", "farro"],
            &["vegetarian"],
            &["gluten", "dairy"],
            29.0,
            13.0,
            82.0,
            88.0,
        ),
        menu_item(
            "herb-chicken",
            "Herb-Roasted Chicken",
            "Half chicken, parsnip, rosemary jus",
            MenuSection::Main,
            &["chicken", "parsnip"],
            &["gluten-free"],
            &[],
            32.0,
            18.0,
            89.0,
            76.0,
        ),
        menu_item(
            "ember-steak",
            "Ember Dry-Aged Steak",
            "Dry-aged strip, smoked onion, pepper jus",
            MenuSection::Main,
            &["beef"],
            &["gluten-free"],
            &[],
            48.0,
            22.0,
            97.0,
            68.0,
        ),
        menu_item(
            "cauliflower",
            "Coal-Roasted Cauliflower",
            "Tahini, preserved lemon, herb salad",
            MenuSection::Main,
            &["cauliflower"],
            &["vegan", "vegetarian", "gluten-free"],
            &["sesame"],
            27.0,
            12.0,
            86.0,
            93.0,
        ),
        menu_item(
            "roasted-roots",
            "Ember-Roasted Roots",
            "Carrot, golden beet, parsnip, thyme",
            MenuSection::Side,
            &["carrot", "beet", "parsnip"],
            &["vegan", "vegetarian", "gluten-free"],
            &[],
            13.0,
            9.0,
            78.0,
            91.0,
        ),
        menu_item(
            "chocolate-torte",
            "Dark Chocolate Torte",
            "Bittersweet chocolate, olive oil, sea salt",
            MenuSection::Dessert,
            &["chocolate"],
            &["vegetarian", "gluten-free"],
            &["eggs"],
            14.0,
            5.0,
            92.0,
            89.0,
        ),
    ]
}

pub fn staff() -> Vec<StaffMember> {
    vec![
        StaffMember {
            id: "host-1".into(),
            name: "Ari Kim".into(),
            role: StaffRole::Host,
            initials: "AK".into(),
            section: None,
        },
        StaffMember {
            id: "server-1".into(),
            name: "Mina Cole".into(),
            role: StaffRole::Server,
            initials: "MC".into(),
            section: Some(DiningArea::Main),
        },
        StaffMember {
            id: "server-2".into(),
            name: "Theo Grant".into(),
            role: StaffRole::Server,
            initials: "TG".into(),
            section: Some(DiningArea::Window),
        },
        StaffMember {
            id: "server-3".into(),
            name: "Rosa Diaz".into(),
            role: StaffRole::Server,
            initials: "RD".into(),
            section: Some(DiningArea::Patio),
        },
        StaffMember {
            id: "manager-1".into(),
            name: "Marcus Lee".into(),
            role: StaffRole::Manager,
            initials: "ML".into(),
            section: None,
        },
    ]
}

#[allow(clippy::too_many_arguments)]
fn table(
    id: &str,
    label: &str,
    capacity: u32,
    area: DiningArea,
    status: TableStatus,
    accessible: bool,
    server_id: &str,
    seated_guest_id: Option<&str>,
    seated_at: Option<&str>,
    estimated_available_minutes: f64,
) -> Table {
    Table {
        id: id.into(),
        label: label.into(),
        capacity,
        area,
        status,
        accessible,
        server_id: server_id.into(),
        seated_guest_id: seated_guest_id.map(Into::into),
        seated_at: seated_at.map(Into::into),
        estimated_available_minutes,
    }
}

pub fn tables() -> Vec<Table> {
    use DiningArea::*;
    use TableStatus::*;
    vec![
        table("t1", "T1", 2, Window, Available, true, "server-2", None, None, 0.0),
        table("t2", "T2", 4, Window, Available, true, "server-2", None, None, 0.0),
        table(
            "t3",
            "T3",
            4,
            Main,
            Occupied,
            true,
            "server-1",
            Some("guest-noah"),
            Some("2026-08-09T21:35:00.000Z"),
            54.0,
        ),
        table("t4", "T4", 6, Main, Available, true, "server-1", None, None, 0.0),
        table("t5", "T5", 4, Main, Clearing, false, "server-1", None, None, 8.0),
        table("t6", "T6", 2, Bar, Available, false, "server-2", None, None, 0.0),
        table("t7", "T7", 4, Patio, Available, true, "server-3", None, None, 0.0),
        table("t8", "T8", 6, Patio, Reserved, true, "server-3", None, None, 25.0),
        table("t9", "T9", 8, Main, Available, true, "server-1", None, None, 0.0),
    ]
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).into()).collect()
}

pub fn guests() -> Vec<GuestProfile> {
    vec![
        GuestProfile {
            id: "guest-maya".into(),
            name: "Maya Chen".into(),
            party_size: 4,
            reservation_time: Some("6:15 PM".into()),
            arrival_time: Some("6:07 PM".into()),
            status: GuestStatus::Waiting,
            allergies: strings(&["tree nuts"]),
            dietary_needs: strings(&["gluten-free"]),
            likes: strings(&["salmon", "citrus", "window"]),
            dislikes: strings(&["mushroom"]),
            seating_preferences: strings(&["window", "accessible"]),
            visit_count: 6,
            last_visit: Some("Jun 18".into()),
            notes: "Celebrating an anniversary. Prefers a quieter table.".into(),
        },
        GuestProfile {
            id: "guest-jordan".into(),
            name: "Jordan Ellis".into(),
            party_size: 2,
            reservation_time: Some("6:30 PM".into()),
            arrival_time: None,
            status: GuestStatus::Expected,
            allergies: strings(&[]),
            dietary_needs: strings(&["vegan"]),
            likes: strings(&["beet", "spicy"]),
            dislikes: strings(&[]),
            seating_preferences: strings(&["patio"]),
            visit_count: 2,
            last_visit: Some("Apr 02".into()),
            notes: "Ask whether patio weather is comfortable.".into(),
        },
        GuestProfile {
            id: "guest-priya".into(),
            name: "Priya Shah".into(),
            party_size: 5,
            reservation_time: Some("7:00 PM".into()),
            arrival_time: Some("6:11 PM".into()),
            status: GuestStatus::Waiting,
            allergies: strings(&["sesame"]),
            dietary_needs: strings(&[]),
            likes: strings(&["chicken", "chef specials"]),
            dislikes: strings(&["fish"]),
            seating_preferences: strings(&["accessible"]),
            visit_count: 1,
            last_visit: None,
            notes: "One guest uses a wheelchair.".into(),
        },
        GuestProfile {
            id: "guest-noah".into(),
            name: "Noah Williams".into(),
            party_size: 3,
            reservation_time: Some("5:45 PM".into()),
            arrival_time: Some("5:39 PM".into()),
            status: GuestStatus::Seated,
            allergies: strings(&[]),
            dietary_needs: strings(&[]),
            likes: strings(&["steak", "chocolate"]),
            dislikes: strings(&[]),
            seating_preferences: strings(&[]),
            visit_count: 4,
            last_visit: Some("May 21".into()),
            notes: "Business dinner; keep pacing efficient.".into(),
        },
    ]
}

pub fn orders() -> Vec<Order> {
    vec![Order {
        id: "order-noah".into(),
        guest_id: "guest-noah".into(),
        table_id: Some("t3".into()),
        status: OrderStatus::Draft,
        lines: vec![],
        guest_notes: String::new(),
        created_at: "2026-08-09T21:39:00.000Z".into(),
        sent_at: None,
        completed_at: None,
    }]
}

/// The starting state for a fresh demo service.
/// Equivalent to `createInitialPosState()` in `components/pos-provider.tsx`.
pub fn initial_state() -> PosState {
    PosState {
        tables: tables(),
        guests: guests(),
        orders: orders(),
        activity: vec![],
        ingredients: ingredients(),
    }
}
