//! Scoring and eligibility rules. Ported from `lib/decision-engine.ts`.
//!
//! The weights and the order in which reasons and warnings are pushed are
//! deliberately identical to the TypeScript original — the tests in this module
//! are the same fixtures the vitest suite used, so any drift shows up as a
//! failure rather than as a quietly different ranking.
//!
//! Everything here is a pure function. Hard safety rules (allergens, dietary
//! conflicts, unavailable stock) live in `recommend_dishes` and gate
//! `eligible`; nothing downstream — including any model in `services/brain` —
//! is permitted to reverse that decision.

use std::collections::HashMap;

use crate::domain::*;

fn normalize(value: &str) -> String {
    value.trim().to_lowercase()
}

/// `Math.max(0, Math.min(100, Math.round(score)))` from the original.
///
/// The bounds are literals, so `clamp` cannot panic here.
fn clamp_score(score: f64) -> f64 {
    score.round().clamp(0.0, 100.0)
}

/// Renders a float the way a JS template literal would, so that reason strings
/// read "Ready in about 8 min" rather than "Ready in about 8 min".
fn format_number(value: f64) -> String {
    if value.is_finite() && value.fract() == 0.0 {
        format!("{}", value as i64)
    } else {
        format!("{value}")
    }
}

pub fn can_seat_guest_at_table(guest: &GuestProfile, table: &Table) -> bool {
    let may_be_seated = matches!(
        guest.status,
        GuestStatus::Waiting | GuestStatus::Seated | GuestStatus::Ordered
    );
    let needs_accessible = guest
        .seating_preferences
        .iter()
        .any(|preference| preference == "accessible");

    may_be_seated
        && table.status == TableStatus::Available
        && table.seated_guest_id.is_none()
        && table.capacity >= guest.party_size
        && (!needs_accessible || table.accessible)
}

pub fn recommend_tables(guest: &GuestProfile, tables: &[Table]) -> Vec<Recommendation> {
    let mut server_loads: HashMap<&str, f64> = HashMap::new();
    for table in tables {
        let entry = server_loads.entry(table.server_id.as_str()).or_insert(0.0);
        *entry += if table.status == TableStatus::Occupied {
            1.0
        } else {
            0.0
        };
    }

    let needs_accessible = guest
        .seating_preferences
        .iter()
        .any(|preference| preference == "accessible");

    let mut recommendations: Vec<Recommendation> = tables
        .iter()
        .map(|table| {
            let mut reasons: Vec<String> = Vec::new();
            let mut warnings: Vec<String> = Vec::new();

            let is_available_soon = table.status == TableStatus::Available
                || (table.status == TableStatus::Clearing
                    && table.estimated_available_minutes <= 15.0);
            let eligible = table.capacity >= guest.party_size
                && is_available_soon
                && (!needs_accessible || table.accessible);

            if table.capacity < guest.party_size {
                warnings.push("Too small for this party".into());
            }
            if !is_available_soon {
                warnings.push("Not available within 15 minutes".into());
            }
            if needs_accessible && !table.accessible {
                warnings.push("Does not meet accessibility need".into());
            }

            let load = server_loads
                .get(table.server_id.as_str())
                .copied()
                .unwrap_or(0.0);
            let spare_seats = table.capacity as i64 - guest.party_size as i64;

            let mut score = 100.0;
            score -= spare_seats.max(0) as f64 * 7.0;
            score -= load * 8.0;
            score -= table.estimated_available_minutes * 1.2;

            if table.capacity == guest.party_size {
                reasons.push("Exact fit for the party".into());
            } else if table.capacity > guest.party_size {
                reasons.push(format!(
                    "{spare_seats} spare seat{}",
                    if spare_seats == 1 { "" } else { "s" }
                ));
            }

            if guest
                .seating_preferences
                .iter()
                .any(|preference| preference == table.area.as_str())
            {
                score += 16.0;
                reasons.push(format!("Matches {} preference", table.area.as_str()));
            }
            if needs_accessible && table.accessible {
                score += 14.0;
                reasons.push("Accessible route and seating".into());
            }
            if load == 0.0 {
                score += 8.0;
                reasons.push("Balances server workload".into());
            }
            if table.status == TableStatus::Available {
                reasons.push("Ready now".into());
            }
            if table.status == TableStatus::Clearing {
                reasons.push(format!(
                    "Ready in about {} min",
                    format_number(table.estimated_available_minutes)
                ));
            }

            Recommendation {
                id: table.id.clone(),
                score: if eligible { clamp_score(score) } else { 0.0 },
                eligible,
                reasons,
                warnings,
            }
        })
        .collect();

    // Stable sort, matching JS: eligible first, then score descending.
    recommendations.sort_by(|a, b| {
        b.eligible
            .cmp(&a.eligible)
            .then(b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal))
    });
    recommendations
}

fn dietary_conflict(item_tags: &[String], need: &str) -> bool {
    let has = |tag: &str| item_tags.iter().any(|value| value == tag);
    match normalize(need).as_str() {
        "vegan" => !has("vegan"),
        "vegetarian" => !has("vegetarian") && !has("vegan"),
        "gluten-free" => !has("gluten-free"),
        _ => false,
    }
}

pub fn recommend_dishes(
    guest: &GuestProfile,
    menu_items: &[MenuItem],
    ingredients: &[Ingredient],
) -> Vec<Recommendation> {
    let mut recommendations: Vec<Recommendation> = menu_items
        .iter()
        .map(|item| {
            let mut reasons: Vec<String> = Vec::new();
            let mut warnings: Vec<String> = Vec::new();

            let normalized_allergens: Vec<String> =
                item.allergens.iter().map(|value| normalize(value)).collect();
            let allergy_matches: Vec<&String> = guest
                .allergies
                .iter()
                .filter(|allergy| normalized_allergens.contains(&normalize(allergy)))
                .collect();
            let dietary_conflicts: Vec<&String> = guest
                .dietary_needs
                .iter()
                .filter(|need| dietary_conflict(&item.tags, need))
                .collect();

            let item_ingredients: Vec<&Ingredient> = ingredients
                .iter()
                .filter(|ingredient| item.ingredient_ids.contains(&ingredient.id))
                .collect();
            let unavailable: Vec<&&Ingredient> = item_ingredients
                .iter()
                .filter(|ingredient| ingredient.on_hand <= 0.0)
                .collect();
            let low_stock: Vec<&&Ingredient> = item_ingredients
                .iter()
                .filter(|ingredient| {
                    ingredient.on_hand > 0.0 && ingredient.on_hand / ingredient.par <= 0.25
                })
                .collect();

            let eligible = allergy_matches.is_empty()
                && dietary_conflicts.is_empty()
                && unavailable.is_empty();

            for allergy in &allergy_matches {
                warnings.push(format!("Contains guest allergen: {allergy}"));
            }
            for need in &dietary_conflicts {
                warnings.push(format!("Does not meet {need}"));
            }
            for ingredient in &unavailable {
                warnings.push(format!("{} is unavailable", ingredient.name));
            }
            for ingredient in &low_stock {
                warnings.push(format!("{} is running low", ingredient.name));
            }

            let mut score = item.popularity * 0.42 + item.margin_score * 0.22;
            score += (22.0 - item.prep_minutes).max(0.0) * 0.65;

            let search_text = format!(
                "{} {} {}",
                item.name,
                item.description,
                item.tags.join(" ")
            )
            .to_lowercase();

            let matched_likes: Vec<&String> = guest
                .likes
                .iter()
                .filter(|like| search_text.contains(&normalize(like)))
                .collect();
            let matched_dislikes: Vec<&String> = guest
                .dislikes
                .iter()
                .filter(|dislike| search_text.contains(&normalize(dislike)))
                .collect();

            if !matched_likes.is_empty() {
                score += 18.0;
                reasons.push(format!(
                    "Matches preference: {}",
                    matched_likes
                        .iter()
                        .map(|value| value.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            if !matched_dislikes.is_empty() {
                score -= 35.0;
                warnings.push(format!(
                    "Guest dislikes {}",
                    matched_dislikes
                        .iter()
                        .map(|value| value.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            if !guest.dietary_needs.is_empty() && dietary_conflicts.is_empty() {
                score += 10.0;
                reasons.push(format!("Meets {}", guest.dietary_needs.join(" + ")));
            }
            if item.prep_minutes <= 12.0 {
                reasons.push("Fast kitchen pacing".into());
            }
            if item.popularity >= 90.0 {
                reasons.push("Guest favorite".into());
            }
            if item.margin_score >= 88.0 {
                reasons.push("Strong value for the restaurant".into());
            }
            if !low_stock.is_empty() {
                score -= 18.0;
            }

            reasons.truncate(3);

            Recommendation {
                id: item.id.clone(),
                score: if eligible { clamp_score(score) } else { 0.0 },
                eligible,
                reasons,
                warnings,
            }
        })
        .collect();

    recommendations.sort_by(|a, b| {
        b.eligible
            .cmp(&a.eligible)
            .then(b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal))
    });
    recommendations
}

pub fn order_total(order: Option<&Order>, menu_items: &[MenuItem]) -> f64 {
    let Some(order) = order else { return 0.0 };
    order
        .lines
        .iter()
        .map(|line| {
            let price = menu_items
                .iter()
                .find(|item| item.id == line.menu_item_id)
                .map(|item| item.price)
                .unwrap_or(0.0);
            price * line.quantity as f64
        })
        .sum()
}

pub fn estimate_wait(guest: &GuestProfile, tables: &[Table]) -> f64 {
    let recommendations = recommend_tables(guest, tables);
    let Some(recommendation) = recommendations.iter().find(|item| item.eligible) else {
        return 25.0;
    };
    match tables.iter().find(|table| table.id == recommendation.id) {
        Some(table) if table.status == TableStatus::Available => 0.0,
        Some(table) => table.estimated_available_minutes,
        None => 15.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seed;

    fn guest(id: &str) -> GuestProfile {
        seed::guests()
            .into_iter()
            .find(|guest| guest.id == id)
            .expect("seeded guest")
    }

    fn find<'a>(items: &'a [Recommendation], id: &str) -> &'a Recommendation {
        items
            .iter()
            .find(|item| item.id == id)
            .expect("recommendation present")
    }

    // --- table recommendations (ported from lib/decision-engine.test.ts) ---

    #[test]
    fn ranks_the_exact_fit_accessible_window_table_first_for_maya() {
        let recommendations = recommend_tables(&guest("guest-maya"), &seed::tables());
        let top = &recommendations[0];

        assert_eq!(top.id, "t2");
        assert!(top.eligible);
        assert_eq!(top.score, 100.0);
        assert!(top
            .reasons
            .contains(&"Matches window preference".to_string()));
    }

    #[test]
    fn enforces_capacity_and_accessibility_as_hard_constraints() {
        let recommendations = recommend_tables(&guest("guest-priya"), &seed::tables());

        // t5 is not accessible; t1 seats two and Priya is a party of five.
        assert!(!find(&recommendations, "t5").eligible);
        assert!(!find(&recommendations, "t1").eligible);
    }

    #[test]
    fn returns_no_wait_when_the_top_table_is_already_available() {
        assert_eq!(estimate_wait(&guest("guest-maya"), &seed::tables()), 0.0);
    }

    #[test]
    fn only_allows_a_checked_in_guest_at_an_available_compatible_table() {
        let tables = seed::tables();
        let accessible = tables.iter().find(|table| table.id == "t2").unwrap();
        let occupied = tables.iter().find(|table| table.id == "t3").unwrap();

        assert!(can_seat_guest_at_table(&guest("guest-maya"), accessible));
        // Jordan is still "expected" — not checked in yet.
        assert!(!can_seat_guest_at_table(&guest("guest-jordan"), accessible));
        assert!(!can_seat_guest_at_table(&guest("guest-maya"), occupied));
    }

    // --- dish recommendations ---

    #[test]
    fn blocks_explicit_allergens_and_unmet_dietary_requirements() {
        let recommendations =
            recommend_dishes(&guest("guest-maya"), &seed::menu_items(), &seed::ingredients());

        let tartare = find(&recommendations, "carrot-tartare");
        assert!(!tartare.eligible);
        assert!(tartare
            .warnings
            .contains(&"Contains guest allergen: tree nuts".to_string()));

        let farro = find(&recommendations, "mushroom-farro");
        assert!(!farro.eligible);
        assert!(farro
            .warnings
            .contains(&"Does not meet gluten-free".to_string()));
    }

    #[test]
    fn keeps_only_vegan_compatible_dishes_eligible_for_a_vegan_guest() {
        let recommendations =
            recommend_dishes(&guest("guest-jordan"), &seed::menu_items(), &seed::ingredients());

        assert!(find(&recommendations, "cauliflower").eligible);
        assert!(!find(&recommendations, "herb-chicken").eligible);
    }

    #[test]
    fn ineligible_dishes_always_score_zero() {
        let recommendations =
            recommend_dishes(&guest("guest-maya"), &seed::menu_items(), &seed::ingredients());

        for recommendation in recommendations.iter().filter(|item| !item.eligible) {
            assert_eq!(recommendation.score, 0.0, "{} scored above zero", recommendation.id);
        }
    }

    // --- orders ---

    #[test]
    fn calculates_totals_from_quantity_and_menu_price() {
        let mut order = seed::orders().remove(0);
        order.lines = vec![
            OrderLine {
                menu_item_id: "beet-salad".into(),
                quantity: 2,
                notes: String::new(),
            },
            OrderLine {
                menu_item_id: "chocolate-torte".into(),
                quantity: 1,
                notes: String::new(),
            },
        ];

        assert_eq!(order_total(Some(&order), &seed::menu_items()), 48.0);
    }

    #[test]
    fn an_absent_order_totals_zero() {
        assert_eq!(order_total(None, &seed::menu_items()), 0.0);
    }
}
