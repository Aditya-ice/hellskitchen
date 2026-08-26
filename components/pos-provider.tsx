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
import type {
  ActivityEvent,
  GuestProfile,
  Ingredient,
  MenuItem,
  Order,
  Recommendation,
  Restaurant,
  StaffMember,
  Table,
} from "@/lib/domain";
import type { Action } from "@/lib/generated/Action";
import {
  ensureDemoSession,
  fetchMenu,
  fetchForecast,
  fetchRecommendations,
  fetchSummary,
  newAction,
  newWalkIn,
  postAction,
  subscribeToState,
  type FloorSummary,
  type Forecast,
  type MenuPayload,
  type Revision,
} from "@/lib/pos-client";

/**
 * The POS no longer holds state — `ember-server` does. This provider keeps a
 * local mirror of the server's latest revision, posts actions, and applies
 * whatever the server pushes back over SSE.
 *
 * The shape of this context is unchanged from the localStorage version, so the
 * views on top of it did not have to be rewritten. What changed underneath:
 * every open surface now shares one floor, and state survives a reload.
 */

/** Everything the server tells us about the currently selected guest. */
export interface GuestInsight {
  /** Which guest these scores belong to; null before the first response. */
  guestId: string | null;
  tables: Recommendation[];
  dishes: Recommendation[];
  estimateWait: number;
  orderTotal: number;
  /** Whether the brain reranked this, or it is the engine's own ordering. */
  rankedBy: "engine" | "model";
}

const emptyInsight: GuestInsight = {
  guestId: null,
  tables: [],
  dishes: [],
  estimateWait: 0,
  orderTotal: 0,
  rankedBy: "engine",
};

const emptySummary: FloorSummary = {
  version: -1,
  waitingGuests: 0,
  openTables: 0,
  averageWaitMinutes: 0,
};

const emptyMenu: MenuPayload = {
  restaurant: {
    name: "Ember & Ash",
    shortName: "E&A",
    venue: "",
    serviceLabel: "Dinner service",
    covers: 0,
  },
  menuItems: [],
  staff: [],
};

interface PosContextValue {
  // state mirrored from the server
  tables: Table[];
  guests: GuestProfile[];
  orders: Order[];
  activity: ActivityEvent[];

  hydrated: boolean;
  /** False while the event stream is down; the UI keeps working read-only. */
  connected: boolean;
  error: string | null;

  // reference data, served by the same Rust seed the engine scores against
  restaurant: Restaurant;
  menuItems: MenuItem[];
  staff: StaffMember[];
  /** Live stock — part of the state, so it moves as tickets are fired. */
  ingredients: Ingredient[];

  /** Server-computed scores for the selected guest. */
  insight: GuestInsight;
  /** Server-computed floor numbers for the header. */
  summary: FloorSummary;
  /** Demand forecast from the optional brain; unavailable without it. */
  forecast: Forecast;

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
  /** Bumped from the pass. Addressed by order id — the kitchen works from
   *  tickets, not from who is sitting where. */
  completeOrder: (orderId: string) => void;
  /** Books a delivery in. Additive, so concurrent restocks add up. */
  restockIngredient: (ingredientId: string, quantity: number) => void;
  resetDemo: () => void;
}

const PosContext = createContext<PosContextValue | null>(null);

const emptyRevision: Revision = {
  version: -1,
  state: { tables: [], guests: [], orders: [], activity: [], ingredients: [] },
};

export function PosProvider({ children }: { children: React.ReactNode }) {
  const [revision, setRevision] = useState<Revision>(emptyRevision);
  const [menu, setMenu] = useState<MenuPayload>(emptyMenu);
  const [insight, setInsight] = useState<GuestInsight>(emptyInsight);
  const [summary, setSummary] = useState<FloorSummary>(emptySummary);
  const [forecast, setForecast] = useState<Forecast>({ available: false });
  const [selectedGuestId, setSelectedGuestId] = useState<string | null>(null);
  const [hydrated, setHydrated] = useState(false);
  const [connected, setConnected] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Guards against an in-flight POST response landing after a newer SSE frame.
  const version = useRef(-1);

  const applyRevision = useCallback((next: Revision) => {
    if (next.version <= version.current) return;
    version.current = next.version;
    setRevision(next);
    setHydrated(true);
  }, []);

  // Reference data is static for the life of the service.
  useEffect(() => {
    const controller = new AbortController();
    fetchMenu(controller.signal)
      .then(setMenu)
      .catch((caught: unknown) => {
        if (controller.signal.aborted) return;
        setError(
          caught instanceof Error ? caught.message : "Could not load the menu.",
        );
      });
    return () => controller.abort();
  }, []);

  // The stream replays the current revision on connect, so this both hydrates
  // and keeps us live. No separate initial fetch is needed.
  useEffect(() => {
    return subscribeToState({
      onRevision: (next) => {
        applyRevision(next);
        setError(null);
      },
      onConnectedChange: setConnected,
    });
  }, [applyRevision]);

  const state = revision.state;

  const effectiveSelectedGuestId = state.guests.some(
    (guest) => guest.id === selectedGuestId,
  )
    ? selectedGuestId
    : (state.guests[0]?.id ?? null);

  // Rescore whenever the selection changes or the floor moves. Over loopback
  // this is sub-millisecond, so it is cheaper than mirroring the engine here.
  useEffect(() => {
    if (!effectiveSelectedGuestId || revision.version < 0) return;

    const controller = new AbortController();
    fetchRecommendations(effectiveSelectedGuestId, controller.signal)
      .then((payload) =>
        setInsight({
          guestId: payload.guestId,
          tables: payload.tables,
          dishes: payload.dishes,
          estimateWait: payload.estimateWait,
          orderTotal: payload.orderTotal,
          rankedBy: payload.rankedBy ?? "engine",
        }),
      )
      .catch(() => {
        // A failed rescore leaves the previous ranking on screen rather than
        // blanking it; the next revision retries.
      });
    return () => controller.abort();
  }, [effectiveSelectedGuestId, revision.version]);

  // Floor numbers follow the floor, not the selection.
  useEffect(() => {
    if (revision.version < 0) return;
    const controller = new AbortController();
    fetchSummary(controller.signal)
      .then(setSummary)
      .catch(() => {
        // Header numbers are not worth surfacing an error over; the next
        // revision retries.
      });
    return () => controller.abort();
  }, [revision.version]);

  // Stock forecasts move over minutes, not seconds, so this is on a timer
  // rather than on every revision — the brain is optional and should not be
  // asked a question per keystroke.
  useEffect(() => {
    const controller = new AbortController();
    const load = () =>
      fetchForecast(controller.signal)
        .then(setForecast)
        .catch(() => {
          // An unavailable forecast is the normal case without a brain.
        });
    load();
    const timer = window.setInterval(load, 60_000);
    return () => {
      controller.abort();
      window.clearInterval(timer);
    };
  }, []);

  // Scores are only shown against the guest they were computed for. While a
  // newly selected guest is being scored the panel is empty rather than
  // showing the previous guest's — those rankings encode someone else's
  // allergies, and a stale one on screen is worse than none.
  const activeInsight =
    insight.guestId && insight.guestId === effectiveSelectedGuestId
      ? insight
      : emptyInsight;

  const dispatch = useCallback(
    (action: Action) => {
      postAction(action)
        .then((outcome) => {
          applyRevision(outcome);
          setError(null);
        })
        .catch((caught: unknown) => {
          setError(
            caught instanceof Error
              ? caught.message
              : "That change could not be saved.",
          );
        });
    },
    [applyRevision],
  );

  const selectGuest = useCallback((id: string) => setSelectedGuestId(id), []);

  const checkInGuest = useCallback(
    (guestId: string) => {
      setSelectedGuestId(guestId);
      dispatch(newAction({ type: "check-in", guestId }));
    },
    [dispatch],
  );

  const addWalkIn = useCallback(
    (name: string, partySize: number) => {
      const guest = newWalkIn(name, partySize);
      setSelectedGuestId(guest.id);
      dispatch(newAction({ type: "add-walk-in", guest }));
      return guest.id;
    },
    [dispatch],
  );

  const updateGuestNotes = useCallback(
    (guestId: string, notes: string) =>
      dispatch(
        newAction({ type: "update-guest-notes", guestId, notes }),
      ),
    [dispatch],
  );

  const seatGuest = useCallback(
    (guestId: string, tableId: string) =>
      dispatch(
        newAction({ type: "seat-guest", guestId, tableId }),
      ),
    [dispatch],
  );

  const addOrderItem = useCallback(
    (guestId: string, menuItemId: string) =>
      dispatch(
        newAction({ type: "add-order-item", guestId, menuItemId }),
      ),
    [dispatch],
  );

  const removeOrderItem = useCallback(
    (guestId: string, menuItemId: string) =>
      dispatch(
        newAction({ type: "remove-order-item", guestId, menuItemId }),
      ),
    [dispatch],
  );

  const updateOrderNotes = useCallback(
    (guestId: string, notes: string) =>
      dispatch(
        newAction({ type: "update-order-notes", guestId, notes }),
      ),
    [dispatch],
  );

  const sendOrder = useCallback(
    (guestId: string) =>
      dispatch(newAction({ type: "send-order", guestId })),
    [dispatch],
  );

  const completeOrder = useCallback(
    (orderId: string) => dispatch(newAction({ type: "complete-order", orderId })),
    [dispatch],
  );

  const restockIngredient = useCallback(
    (ingredientId: string, quantity: number) =>
      dispatch(newAction({ type: "restock-ingredient", ingredientId, quantity })),
    [dispatch],
  );

  const resetDemo = useCallback(() => {
    setSelectedGuestId(null);
    dispatch(newAction({ type: "reset" }));
  }, [dispatch]);

  // Sponsor routes need a session cookie; ask for it once, up front, so the
  // first voice note does not pay for the round trip.
  useEffect(() => {
    ensureDemoSession().catch(() => {
      // Voice and dish context fall back on their own if this never succeeds.
    });
  }, []);

  const value = useMemo<PosContextValue>(
    () => ({
      tables: state.tables,
      guests: state.guests,
      orders: state.orders,
      activity: state.activity,
      hydrated,
      connected,
      error,
      restaurant: menu.restaurant,
      menuItems: menu.menuItems,
      staff: menu.staff,
      ingredients: state.ingredients,
      insight: activeInsight,
      summary,
      forecast,
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
      completeOrder,
      restockIngredient,
      resetDemo,
    }),
    [
      state,
      hydrated,
      connected,
      error,
      menu,
      activeInsight,
      summary,
      forecast,
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
      completeOrder,
      restockIngredient,
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
