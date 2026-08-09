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
  selectedGuestId: string | null;
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
