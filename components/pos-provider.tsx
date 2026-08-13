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
import { canSeatGuestAtTable } from "@/lib/decision-engine";
import type {
  ActivityEvent,
  GuestProfile,
  PosState,
} from "@/lib/domain";

const STORAGE_KEY = "ember-pos-state-v2";
const CHANNEL_KEY = "ember-pos-live-v2";

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
    orders: demoOrders.map((order) => ({
      ...order,
      lines: order.lines.map((line) => ({ ...line })),
    })),
    activity: [],
  };
}

type SharedAction =
  | { id: string; at: string; type: "check-in"; guestId: string }
  | { id: string; at: string; type: "add-walk-in"; guest: GuestProfile }
  | { id: string; at: string; type: "update-guest-notes"; guestId: string; notes: string }
  | { id: string; at: string; type: "seat-guest"; guestId: string; tableId: string }
  | { id: string; at: string; type: "add-order-item"; guestId: string; menuItemId: string }
  | { id: string; at: string; type: "remove-order-item"; guestId: string; menuItemId: string }
  | { id: string; at: string; type: "update-order-notes"; guestId: string; notes: string }
  | { id: string; at: string; type: "send-order"; guestId: string }
  | { id: string; at: string; type: "reset" };

interface SharedMessage {
  action: SharedAction;
}

interface PosContextValue extends PosState {
  hydrated: boolean;
  selectedGuestId: string | null;
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

function activity(action: SharedAction, label: string, detail: string): ActivityEvent {
  return {
    id: `${action.id}-activity`,
    at: action.at,
    action: label,
    detail,
  };
}

function reducePosState(current: PosState, action: SharedAction): PosState {
  switch (action.type) {
    case "check-in": {
      const guest = current.guests.find((item) => item.id === action.guestId);
      if (!guest || guest.status !== "expected") return current;
      return {
        ...current,
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
      return freshState();
  }
}

function parseStoredState(value: string | null): PosState | null {
  if (!value) return null;
  try {
    const parsed = JSON.parse(value) as Partial<PosState>;
    if (
      !Array.isArray(parsed.tables) ||
      !Array.isArray(parsed.guests) ||
      !Array.isArray(parsed.orders) ||
      !Array.isArray(parsed.activity)
    ) {
      return null;
    }
    return {
      tables: parsed.tables,
      guests: parsed.guests,
      orders: parsed.orders,
      activity: parsed.activity,
    };
  } catch {
    return null;
  }
}

function persist(state: PosState) {
  try {
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
  } catch {
    // The POS remains usable when storage is unavailable or full.
  }
}

function newAction<T extends Omit<SharedAction, "id" | "at">>(
  action: T,
): T & { id: string; at: string } {
  return {
    ...action,
    id: crypto.randomUUID(),
    at: new Date().toISOString(),
  };
}

export function PosProvider({ children }: { children: React.ReactNode }) {
  const [state, setState] = useState<PosState>(freshState);
  const [selectedGuestId, setSelectedGuestId] = useState<string | null>(
    demoGuests[0]?.id ?? null,
  );
  const [hydrated, setHydrated] = useState(false);
  const channel = useRef<BroadcastChannel | null>(null);
  const seenActions = useRef(new Set<string>());

  useEffect(() => {
    let savedState: PosState | null = null;
    try {
      savedState = parseStoredState(window.localStorage.getItem(STORAGE_KEY));
    } catch {
      savedState = null;
    }

    if ("BroadcastChannel" in window) {
      channel.current = new BroadcastChannel(CHANNEL_KEY);
      channel.current.onmessage = (message: MessageEvent<SharedMessage>) => {
        const action = message.data?.action;
        if (!action?.id || seenActions.current.has(action.id)) return;
        seenActions.current.add(action.id);
        setState((current) => {
          const next = reducePosState(current, action);
          persist(next);
          return next;
        });
      };
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

  const dispatchShared = useCallback((action: SharedAction) => {
    seenActions.current.add(action.id);
    setState((current) => {
      const next = reducePosState(current, action);
      persist(next);
      return next;
    });
    channel.current?.postMessage({ action } satisfies SharedMessage);
  }, []);

  const effectiveSelectedGuestId = state.guests.some(
    (guest) => guest.id === selectedGuestId,
  )
    ? selectedGuestId
    : state.guests[0]?.id ?? null;

  const selectGuest = useCallback((id: string) => setSelectedGuestId(id), []);

  const checkInGuest = useCallback(
    (guestId: string) => {
      setSelectedGuestId(guestId);
      dispatchShared(newAction({ type: "check-in", guestId }));
    },
    [dispatchShared],
  );

  const addWalkIn = useCallback(
    (name: string, partySize: number) => {
      const actionId = crypto.randomUUID();
      const at = new Date().toISOString();
      const guest: GuestProfile = {
        id: `guest-${actionId}`,
        name,
        partySize,
        reservationTime: null,
        arrivalTime: new Date(at).toLocaleTimeString([], {
          hour: "numeric",
          minute: "2-digit",
        }),
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
      setSelectedGuestId(guest.id);
      dispatchShared({ id: actionId, at, type: "add-walk-in", guest });
      return guest.id;
    },
    [dispatchShared],
  );

  const updateGuestNotes = useCallback(
    (guestId: string, notes: string) =>
      dispatchShared(newAction({ type: "update-guest-notes", guestId, notes })),
    [dispatchShared],
  );

  const seatGuest = useCallback(
    (guestId: string, tableId: string) =>
      dispatchShared(newAction({ type: "seat-guest", guestId, tableId })),
    [dispatchShared],
  );

  const addOrderItem = useCallback(
    (guestId: string, menuItemId: string) =>
      dispatchShared(newAction({ type: "add-order-item", guestId, menuItemId })),
    [dispatchShared],
  );

  const removeOrderItem = useCallback(
    (guestId: string, menuItemId: string) =>
      dispatchShared(
        newAction({ type: "remove-order-item", guestId, menuItemId }),
      ),
    [dispatchShared],
  );

  const updateOrderNotes = useCallback(
    (guestId: string, notes: string) =>
      dispatchShared(newAction({ type: "update-order-notes", guestId, notes })),
    [dispatchShared],
  );

  const sendOrder = useCallback(
    (guestId: string) =>
      dispatchShared(newAction({ type: "send-order", guestId })),
    [dispatchShared],
  );

  const resetDemo = useCallback(() => {
    setSelectedGuestId(demoGuests[0]?.id ?? null);
    dispatchShared(newAction({ type: "reset" }));
  }, [dispatchShared]);

  const value = useMemo<PosContextValue>(
    () => ({
      ...state,
      hydrated,
      selectedGuestId: effectiveSelectedGuestId,
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
      effectiveSelectedGuestId,
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
