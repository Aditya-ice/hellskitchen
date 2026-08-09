"use client";

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { demoGuests, demoOrders, demoTables } from "@/data/demo";
import type { ActivityEvent, GuestProfile, PosState } from "@/lib/domain";

const STORAGE_KEY = "ember-pos-state-v1";
const CHANNEL_KEY = "ember-pos-live";

function freshState(): PosState {
  return {
    tables: demoTables.map((table) => ({ ...table })),
    guests: demoGuests.map((guest) => ({
      ...guest,
      allergies: [...guest.allergies],
      dietaryNeeds: [...guest.dietaryNeeds],
      likes: [...guest.likes],
      dislikes: [...guest.dislikes],
      seatingPreferences: [...guest.seatingPreferences],
    })),
    orders: demoOrders.map((order) => ({ ...order, lines: [...order.lines] })),
    selectedGuestId: "guest-maya",
    activity: [],
  };
}

interface PosContextValue extends PosState {
  hydrated: boolean;
  selectGuest: (id: string) => void;
  checkInGuest: (id: string) => void;
  addWalkIn: (name: string, partySize: number) => string;
  updateGuestNotes: (id: string, notes: string) => void;
  seatGuest: (guestId: string, tableId: string) => void;
  addOrderItem: (guestId: string, menuItemId: string) => void;
  removeOrderItem: (guestId: string, menuItemId: string) => void;
  updateOrderNotes: (guestId: string, notes: string) => void;
  sendOrder: (guestId: string) => void;
  resetDemo: () => void;
}

const PosContext = createContext<PosContextValue | null>(null);

function event(action: string, detail: string): ActivityEvent {
  return {
    id: `${Date.now()}-${Math.random().toString(16).slice(2)}`,
    at: new Date().toISOString(),
    action,
    detail,
  };
}

export function PosProvider({ children }: { children: React.ReactNode }) {
  const [state, setState] = useState<PosState>(freshState);
  const [hydrated, setHydrated] = useState(false);
  const channel = useRef<BroadcastChannel | null>(null);

  useEffect(() => {
    let savedState: PosState | null = null;
    try {
      const saved = window.localStorage.getItem(STORAGE_KEY);
      if (saved) savedState = JSON.parse(saved) as PosState;
    } catch {
      window.localStorage.removeItem(STORAGE_KEY);
    }

    if ("BroadcastChannel" in window) {
      channel.current = new BroadcastChannel(CHANNEL_KEY);
      channel.current.onmessage = (message: MessageEvent<PosState>) => setState(message.data);
    }
    const hydrationTimer = window.setTimeout(() => {
      if (savedState) setState(savedState);
      setHydrated(true);
    }, 0);

    return () => {
      window.clearTimeout(hydrationTimer);
      channel.current?.close();
    };
  }, []);

  const commit = useCallback((recipe: (current: PosState) => PosState) => {
    setState((current) => {
      const next = recipe(current);
      window.localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
      channel.current?.postMessage(next);
      return next;
    });
  }, []);

  const selectGuest = useCallback(
    (id: string) => commit((current) => ({ ...current, selectedGuestId: id })),
    [commit],
  );

  const checkInGuest = useCallback(
    (id: string) =>
      commit((current) => {
        const guest = current.guests.find((item) => item.id === id);
        return {
          ...current,
          guests: current.guests.map((item) =>
            item.id === id
              ? { ...item, status: "waiting", arrivalTime: new Date().toLocaleTimeString([], { hour: "numeric", minute: "2-digit" }) }
              : item,
          ),
          selectedGuestId: id,
          activity: [event("Guest checked in", `${guest?.name ?? "Guest"} joined the arrivals queue`), ...current.activity],
        };
      }),
    [commit],
  );

  const addWalkIn = useCallback(
    (name: string, partySize: number) => {
      const id = `guest-${Date.now()}`;
      const guest: GuestProfile = {
        id,
        name,
        partySize,
        reservationTime: null,
        arrivalTime: new Date().toLocaleTimeString([], { hour: "numeric", minute: "2-digit" }),
        status: "waiting",
        allergies: [],
        dietaryNeeds: [],
        likes: [],
        dislikes: [],
        seatingPreferences: [],
        visitCount: 0,
        lastVisit: null,
        notes: "Walk-in guest",
      };
      commit((current) => ({
        ...current,
        guests: [...current.guests, guest],
        selectedGuestId: id,
        activity: [event("Walk-in added", `${name}, party of ${partySize}`), ...current.activity],
      }));
      return id;
    },
    [commit],
  );

  const updateGuestNotes = useCallback(
    (id: string, notes: string) =>
      commit((current) => ({
        ...current,
        guests: current.guests.map((guest) => (guest.id === id ? { ...guest, notes } : guest)),
      })),
    [commit],
  );

  const seatGuest = useCallback(
    (guestId: string, tableId: string) =>
      commit((current) => {
        const guest = current.guests.find((item) => item.id === guestId);
        const currentTable = current.tables.find((table) => table.seatedGuestId === guestId);
        const existingOrder = current.orders.find((order) => order.guestId === guestId);
        const now = new Date().toISOString();
        return {
          ...current,
          tables: current.tables.map((table) => {
            if (table.id === currentTable?.id) {
              return { ...table, status: "available", seatedGuestId: null, seatedAt: null };
            }
            if (table.id === tableId) {
              return { ...table, status: "occupied", seatedGuestId: guestId, seatedAt: now };
            }
            return table;
          }),
          guests: current.guests.map((item) =>
            item.id === guestId ? { ...item, status: "seated" } : item,
          ),
          orders: existingOrder
            ? current.orders.map((order) =>
                order.guestId === guestId ? { ...order, tableId } : order,
              )
            : [
                ...current.orders,
                {
                  id: `order-${Date.now()}`,
                  guestId,
                  tableId,
                  status: "draft",
                  lines: [],
                  guestNotes: "",
                  createdAt: now,
                },
              ],
          activity: [
            event(currentTable ? "Party moved" : "Party seated", `${guest?.name ?? "Guest"} assigned to ${current.tables.find((item) => item.id === tableId)?.label ?? tableId}`),
            ...current.activity,
          ],
        };
      }),
    [commit],
  );

  const addOrderItem = useCallback(
    (guestId: string, menuItemId: string) =>
      commit((current) => ({
        ...current,
        orders: current.orders.map((order) =>
          order.guestId !== guestId
            ? order
            : {
                ...order,
                lines: order.lines.some((line) => line.menuItemId === menuItemId)
                  ? order.lines.map((line) =>
                      line.menuItemId === menuItemId
                        ? { ...line, quantity: line.quantity + 1 }
                        : line,
                    )
                  : [...order.lines, { menuItemId, quantity: 1, notes: "" }],
              },
        ),
      })),
    [commit],
  );

  const removeOrderItem = useCallback(
    (guestId: string, menuItemId: string) =>
      commit((current) => ({
        ...current,
        orders: current.orders.map((order) =>
          order.guestId !== guestId
            ? order
            : {
                ...order,
                lines: order.lines
                  .map((line) =>
                    line.menuItemId === menuItemId
                      ? { ...line, quantity: line.quantity - 1 }
                      : line,
                  )
                  .filter((line) => line.quantity > 0),
              },
        ),
      })),
    [commit],
  );

  const updateOrderNotes = useCallback(
    (guestId: string, guestNotes: string) =>
      commit((current) => ({
        ...current,
        orders: current.orders.map((order) =>
          order.guestId === guestId ? { ...order, guestNotes } : order,
        ),
      })),
    [commit],
  );

  const sendOrder = useCallback(
    (guestId: string) =>
      commit((current) => {
        const guest = current.guests.find((item) => item.id === guestId);
        return {
          ...current,
          orders: current.orders.map((order) =>
            order.guestId === guestId ? { ...order, status: "sent" } : order,
          ),
          guests: current.guests.map((item) =>
            item.id === guestId ? { ...item, status: "ordered" } : item,
          ),
          activity: [event("Order sent", `${guest?.name ?? "Guest"} order sent to kitchen`), ...current.activity],
        };
      }),
    [commit],
  );

  const resetDemo = useCallback(() => commit(() => freshState()), [commit]);

  const value = useMemo<PosContextValue>(
    () => ({
      ...state,
      hydrated,
      selectGuest,
      checkInGuest,
      addWalkIn,
      updateGuestNotes,
      seatGuest,
      addOrderItem,
      removeOrderItem,
      updateOrderNotes,
      sendOrder,
      resetDemo,
    }),
    [
      state,
      hydrated,
      selectGuest,
      checkInGuest,
      addWalkIn,
      updateGuestNotes,
      seatGuest,
      addOrderItem,
      removeOrderItem,
      updateOrderNotes,
      sendOrder,
      resetDemo,
    ],
  );

  return <PosContext.Provider value={value}>{children}</PosContext.Provider>;
}

export function usePos() {
  const context = useContext(PosContext);
  if (!context) throw new Error("usePos must be used inside PosProvider");
  return context;
}
