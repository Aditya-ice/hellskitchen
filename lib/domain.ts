/**
 * Domain types for the POS.
 *
 * These are no longer hand-written: they are generated from the Rust structs
 * in `crates/ember-core/src/domain.rs` by ts-rs (run `cargo test`) and land in
 * `lib/generated/`. This barrel keeps `@/lib/domain` as the import path so the
 * UI does not care where they came from, and makes drift between the two
 * languages impossible.
 */

export type { ActivityEvent } from "@/lib/generated/ActivityEvent";
export type { DiningArea } from "@/lib/generated/DiningArea";
export type { GuestProfile } from "@/lib/generated/GuestProfile";
export type { GuestStatus } from "@/lib/generated/GuestStatus";
export type { Ingredient } from "@/lib/generated/Ingredient";
export type { MenuItem } from "@/lib/generated/MenuItem";
export type { MenuSection } from "@/lib/generated/MenuSection";
export type { Order } from "@/lib/generated/Order";
export type { OrderLine } from "@/lib/generated/OrderLine";
export type { OrderStatus } from "@/lib/generated/OrderStatus";
export type { PosState } from "@/lib/generated/PosState";
export type { Recommendation } from "@/lib/generated/Recommendation";
export type { Restaurant } from "@/lib/generated/Restaurant";
export type { StaffMember } from "@/lib/generated/StaffMember";
export type { StaffRole } from "@/lib/generated/StaffRole";
export type { Table } from "@/lib/generated/Table";
export type { TableStatus } from "@/lib/generated/TableStatus";
export type { TavilyContext } from "@/lib/generated/TavilyContext";
export type { TavilySource } from "@/lib/generated/TavilySource";

/** A scored table suggestion. */
export type { Recommendation as TableRecommendation } from "@/lib/generated/Recommendation";
/** A scored dish suggestion. */
export type { Recommendation as DishRecommendation } from "@/lib/generated/Recommendation";
