export type DiningArea = "main" | "window" | "patio" | "bar";
export type TableStatus = "available" | "occupied" | "clearing" | "reserved";
export type GuestStatus = "expected" | "waiting" | "seated" | "ordered";
export type OrderStatus = "draft" | "sent";

export interface Ingredient {
  id: string;
  name: string;
  aliases: string[];
  onHand: number;
  par: number;
  unit: string;
}

export interface MenuItem {
  id: string;
  name: string;
  description: string;
  section: "starter" | "main" | "side" | "dessert";
  ingredientIds: string[];
  tags: string[];
  allergens: string[];
  price: number;
  prepMinutes: number;
  popularity: number;
  marginScore: number;
}

export interface Table {
  id: string;
  label: string;
  capacity: number;
  area: DiningArea;
  status: TableStatus;
  accessible: boolean;
  serverId: string;
  seatedGuestId: string | null;
  seatedAt: string | null;
  estimatedAvailableMinutes: number;
}

export interface GuestProfile {
  id: string;
  name: string;
  partySize: number;
  reservationTime: string | null;
  arrivalTime: string | null;
  status: GuestStatus;
  allergies: string[];
  dietaryNeeds: string[];
  likes: string[];
  dislikes: string[];
  seatingPreferences: string[];
  visitCount: number;
  lastVisit: string | null;
  notes: string;
}

export interface StaffMember {
  id: string;
  name: string;
  role: "host" | "server" | "manager";
  initials: string;
  section?: DiningArea;
}

export interface OrderLine {
  menuItemId: string;
  quantity: number;
  notes: string;
}

export interface Order {
  id: string;
  guestId: string;
  tableId: string | null;
  status: OrderStatus;
  lines: OrderLine[];
  guestNotes: string;
  createdAt: string;
}

export interface ActivityEvent {
  id: string;
  at: string;
  action: string;
  detail: string;
}

export interface PosState {
  tables: Table[];
  guests: GuestProfile[];
  orders: Order[];
  activity: ActivityEvent[];
}

export interface Recommendation {
  id: string;
  score: number;
  eligible: boolean;
  reasons: string[];
  warnings: string[];
}

export type TableRecommendation = Recommendation;
export type DishRecommendation = Recommendation;

export interface TavilySource {
  title: string;
  url: string;
  content: string;
}

export interface TavilyContext {
  answer: string | null;
  sources: TavilySource[];
  isFallback: boolean;
}

export type SharedAction =
  | { id: string; at: string; type: "check-in"; guestId: string }
  | { id: string; at: string; type: "add-walk-in"; guest: GuestProfile }
  | { id: string; at: string; type: "update-guest-notes"; guestId: string; notes: string }
  | { id: string; at: string; type: "seat-guest"; guestId: string; tableId: string }
  | { id: string; at: string; type: "add-order-item"; guestId: string; menuItemId: string }
  | { id: string; at: string; type: "remove-order-item"; guestId: string; menuItemId: string }
  | { id: string; at: string; type: "update-order-notes"; guestId: string; notes: string }
  | { id: string; at: string; type: "send-order"; guestId: string }
  | { id: string; at: string; type: "reset" };

export type SharedActionInput = SharedAction extends infer Action
  ? Action extends SharedAction
    ? Omit<Action, "id" | "at">
    : never
  : never;
