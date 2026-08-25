import { ingredients, menuItems } from "./demo.js";
import type {
  ActivityEvent,
  DishRecommendation,
  GuestProfile,
  Order,
  PosState,
  SharedAction,
  Table,
  TableRecommendation,
} from "./domain.js";
import { demoGuests, demoOrders, demoTables } from "./demo.js";

const clampScore = (score: number) => Math.max(0, Math.min(100, Math.round(score)));
const normalize = (value: string) => value.trim().toLowerCase();

export function canSeatGuestAtTable(guest: GuestProfile, table: Table) {
  const mayBeSeated = ["waiting", "seated", "ordered"].includes(guest.status);
  const needsAccessible = guest.seatingPreferences.includes("accessible");

  return (
    mayBeSeated &&
    table.status === "available" &&
    table.seatedGuestId === null &&
    table.capacity >= guest.partySize &&
    (!needsAccessible || table.accessible)
  );
}

export function recommendTables(
  guest: GuestProfile,
  tables: Table[],
): TableRecommendation[] {
  const serverLoads = tables.reduce<Record<string, number>>((loads, table) => {
    loads[table.serverId] = (loads[table.serverId] ?? 0) + (table.status === "occupied" ? 1 : 0);
    return loads;
  }, {});

  return tables
    .map((table) => {
      const reasons: string[] = [];
      const warnings: string[] = [];
      const needsAccessible = guest.seatingPreferences.includes("accessible");
      const isAvailableSoon =
        table.status === "available" ||
        (table.status === "clearing" && table.estimatedAvailableMinutes <= 15);
      const eligible =
        table.capacity >= guest.partySize &&
        isAvailableSoon &&
        (!needsAccessible || table.accessible);

      if (table.capacity < guest.partySize) warnings.push("Too small for this party");
      if (!isAvailableSoon) warnings.push("Not available within 15 minutes");
      if (needsAccessible && !table.accessible) warnings.push("Does not meet accessibility need");

      let score = 100;
      const spareSeats = table.capacity - guest.partySize;
      score -= Math.max(0, spareSeats) * 7;
      score -= (serverLoads[table.serverId] ?? 0) * 8;
      score -= table.estimatedAvailableMinutes * 1.2;

      if (table.capacity === guest.partySize) reasons.push("Exact fit for the party");
      else if (table.capacity > guest.partySize) reasons.push(`${spareSeats} spare seat${spareSeats === 1 ? "" : "s"}`);

      if (guest.seatingPreferences.includes(table.area)) {
        score += 16;
        reasons.push(`Matches ${table.area} preference`);
      }
      if (needsAccessible && table.accessible) {
        score += 14;
        reasons.push("Accessible route and seating");
      }
      if ((serverLoads[table.serverId] ?? 0) === 0) {
        score += 8;
        reasons.push("Balances server workload");
      }
      if (table.status === "available") reasons.push("Ready now");
      if (table.status === "clearing") reasons.push(`Ready in about ${table.estimatedAvailableMinutes} min`);

      return {
        id: table.id,
        score: eligible ? clampScore(score) : 0,
        eligible,
        reasons,
        warnings,
      };
    })
    .sort((a, b) => Number(b.eligible) - Number(a.eligible) || b.score - a.score);
}

function dietaryConflict(itemTags: string[], need: string) {
  const value = normalize(need);
  if (value === "vegan") return !itemTags.includes("vegan");
  if (value === "vegetarian") {
    return !itemTags.includes("vegetarian") && !itemTags.includes("vegan");
  }
  if (value === "gluten-free") return !itemTags.includes("gluten-free");
  return false;
}

export function recommendDishes(guest: GuestProfile): DishRecommendation[] {
  return menuItems
    .map((item) => {
      const reasons: string[] = [];
      const warnings: string[] = [];
      const normalizedAllergens = item.allergens.map(normalize);
      const allergyMatches = guest.allergies.filter((allergy) =>
        normalizedAllergens.includes(normalize(allergy)),
      );
      const dietaryConflicts = guest.dietaryNeeds.filter((need) =>
        dietaryConflict(item.tags, need),
      );
      const itemIngredients = ingredients.filter((ingredient) =>
        item.ingredientIds.includes(ingredient.id),
      );
      const unavailable = itemIngredients.filter((ingredient) => ingredient.onHand <= 0);
      const lowStock = itemIngredients.filter(
        (ingredient) => ingredient.onHand > 0 && ingredient.onHand / ingredient.par <= 0.25,
      );
      const eligible =
        allergyMatches.length === 0 &&
        dietaryConflicts.length === 0 &&
        unavailable.length === 0;

      allergyMatches.forEach((allergy) => warnings.push(`Contains guest allergen: ${allergy}`));
      dietaryConflicts.forEach((need) => warnings.push(`Does not meet ${need}`));
      unavailable.forEach((ingredient) => warnings.push(`${ingredient.name} is unavailable`));
      lowStock.forEach((ingredient) => warnings.push(`${ingredient.name} is running low`));

      let score = item.popularity * 0.42 + item.marginScore * 0.22;
      score += Math.max(0, 22 - item.prepMinutes) * 0.65;

      const searchText = `${item.name} ${item.description} ${item.tags.join(" ")}`.toLowerCase();
      const matchedLikes = guest.likes.filter((like) => searchText.includes(normalize(like)));
      const matchedDislikes = guest.dislikes.filter((dislike) => searchText.includes(normalize(dislike)));

      if (matchedLikes.length) {
        score += 18;
        reasons.push(`Matches preference: ${matchedLikes.join(", ")}`);
      }
      if (matchedDislikes.length) {
        score -= 35;
        warnings.push(`Guest dislikes ${matchedDislikes.join(", ")}`);
      }
      if (guest.dietaryNeeds.length && dietaryConflicts.length === 0) {
        score += 10;
        reasons.push(`Meets ${guest.dietaryNeeds.join(" + ")}`);
      }
      if (item.prepMinutes <= 12) reasons.push("Fast kitchen pacing");
      if (item.popularity >= 90) reasons.push("Guest favorite");
      if (item.marginScore >= 88) reasons.push("Strong value for the restaurant");
      if (lowStock.length) score -= 18;

      return {
        id: item.id,
        score: eligible ? clampScore(score) : 0,
        eligible,
        reasons: reasons.slice(0, 3),
        warnings,
      };
    })
    .sort((a, b) => Number(b.eligible) - Number(a.eligible) || b.score - a.score);
}

export function orderTotal(order: Order | undefined) {
  if (!order) return 0;
  return order.lines.reduce((total, line) => {
    const item = menuItems.find((menuItem) => menuItem.id === line.menuItemId);
    return total + (item?.price ?? 0) * line.quantity;
  }, 0);
}

export function estimateWait(guest: GuestProfile, tables: Table[]) {
  const recommendation = recommendTables(guest, tables).find((item) => item.eligible);
  if (!recommendation) return 25;
  const table = tables.find((item) => item.id === recommendation.id);
  return table?.status === "available" ? 0 : table?.estimatedAvailableMinutes ?? 15;
}

export function createInitialPosState(version = 0): PosState {
  return {
    version,
    tables: demoTables.map((table) => ({ ...table })),
    guests: demoGuests.map((guest) => ({
      ...guest,
      allergies: [...guest.allergies],
      dietaryNeeds: [...guest.dietaryNeeds],
      likes: [...guest.likes],
      dislikes: [...guest.dislikes],
      seatingPreferences: [...guest.seatingPreferences],
    })),
    orders: demoOrders.map((order) => ({
      ...order,
      lines: order.lines.map((line) => ({ ...line })),
    })),
    activity: [],
  };
}

function activity(action: SharedAction, label: string, detail: string): ActivityEvent {
  return {
    id: `${action.id}-activity`,
    at: action.at,
    action: label,
    detail,
  };
}

export function reducePosState(current: PosState, action: SharedAction): PosState {
  switch (action.type) {
    case "check-in": {
      const guest = current.guests.find((item) => item.id === action.guestId);
      if (!guest || guest.status !== "expected") return current;
      return {
        ...current,
        version: current.version + 1,
        guests: current.guests.map((item) =>
          item.id === action.guestId
            ? {
                ...item,
                status: "waiting",
                arrivalTime: new Date(action.at).toLocaleTimeString([], {
                  hour: "numeric",
                  minute: "2-digit",
                }),
              }
            : item,
        ),
        activity: [
          activity(action, "Guest checked in", `${guest.name} joined the arrivals queue`),
          ...current.activity,
        ],
      };
    }
    case "add-walk-in":
      if (current.guests.some((guest) => guest.id === action.guest.id)) return current;
      return {
        ...current,
        version: current.version + 1,
        guests: [...current.guests, action.guest],
        activity: [
          activity(
            action,
            "Walk-in added",
            `${action.guest.name}, party of ${action.guest.partySize}`,
          ),
          ...current.activity,
        ],
      };
    case "update-guest-notes":
      return {
        ...current,
        version: current.version + 1,
        guests: current.guests.map((guest) =>
          guest.id === action.guestId ? { ...guest, notes: action.notes } : guest,
        ),
      };
    case "seat-guest": {
      const guest = current.guests.find((item) => item.id === action.guestId);
      const currentTable = current.tables.find(
        (table) => table.seatedGuestId === action.guestId,
      );
      const targetTable = current.tables.find((table) => table.id === action.tableId);
      if (
        !guest ||
        !targetTable ||
        currentTable?.id === targetTable.id ||
        !canSeatGuestAtTable(guest, targetTable)
      ) {
        return current;
      }

      const existingOrder = current.orders.find(
        (order) => order.guestId === action.guestId,
      );
      return {
        ...current,
        version: current.version + 1,
        tables: current.tables.map((table) => {
          if (table.id === currentTable?.id) {
            return {
              ...table,
              status: "available",
              seatedGuestId: null,
              seatedAt: null,
            };
          }
          if (table.id === targetTable.id) {
            return {
              ...table,
              status: "occupied",
              seatedGuestId: action.guestId,
              seatedAt: action.at,
            };
          }
          return table;
        }),
        guests: current.guests.map((item) =>
          item.id === action.guestId ? { ...item, status: "seated" } : item,
        ),
        orders: existingOrder
          ? current.orders.map((order) =>
              order.guestId === action.guestId
                ? { ...order, tableId: targetTable.id }
                : order,
            )
          : [
              ...current.orders,
              {
                id: `order-${action.id}`,
                guestId: action.guestId,
                tableId: targetTable.id,
                status: "draft",
                lines: [],
                guestNotes: "",
                createdAt: action.at,
              },
            ],
        activity: [
          activity(
            action,
            currentTable ? "Party moved" : "Party seated",
            `${guest.name} assigned to ${targetTable.label}`,
          ),
          ...current.activity,
        ],
      };
    }
    case "add-order-item":
      return {
        ...current,
        version: current.version + 1,
        orders: current.orders.map((order) =>
          order.guestId !== action.guestId || order.status === "sent"
            ? order
            : {
                ...order,
                lines: order.lines.some(
                  (line) => line.menuItemId === action.menuItemId,
                )
                  ? order.lines.map((line) =>
                      line.menuItemId === action.menuItemId
                        ? { ...line, quantity: line.quantity + 1 }
                        : line,
                    )
                  : [
                      ...order.lines,
                      { menuItemId: action.menuItemId, quantity: 1, notes: "" },
                    ],
              },
        ),
      };
    case "remove-order-item":
      return {
        ...current,
        version: current.version + 1,
        orders: current.orders.map((order) =>
          order.guestId !== action.guestId || order.status === "sent"
            ? order
            : {
                ...order,
                lines: order.lines
                  .map((line) =>
                    line.menuItemId === action.menuItemId
                      ? { ...line, quantity: line.quantity - 1 }
                      : line,
                  )
                  .filter((line) => line.quantity > 0),
              },
        ),
      };
    case "update-order-notes":
      return {
        ...current,
        version: current.version + 1,
        orders: current.orders.map((order) =>
          order.guestId === action.guestId && order.status === "draft"
            ? { ...order, guestNotes: action.notes }
            : order,
        ),
      };
    case "send-order": {
      const guest = current.guests.find((item) => item.id === action.guestId);
      const order = current.orders.find((item) => item.guestId === action.guestId);
      if (!guest || !order || order.status !== "draft" || order.lines.length === 0) {
        return current;
      }
      return {
        ...current,
        version: current.version + 1,
        orders: current.orders.map((item) =>
          item.id === order.id ? { ...item, status: "sent" } : item,
        ),
        guests: current.guests.map((item) =>
          item.id === action.guestId ? { ...item, status: "ordered" } : item,
        ),
        activity: [
          activity(action, "Order sent", `${guest.name} order sent to kitchen`),
          ...current.activity,
        ],
      };
    }
    case "reset":
      return createInitialPosState(current.version + 1);
  }
}
