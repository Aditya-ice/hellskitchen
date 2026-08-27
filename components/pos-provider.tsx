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
import type { Rejection } from "@/lib/generated/Rejection";
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

/**
 * Something the person at the terminal needs told.
 *
 * `refused` is the common one and is not a fault: the server allowed the
 * request but a guard said no — the table was taken, the ticket is already with
 * the kitchen. `failed` means the change did not reach the server at all, so
 * the floor on screen may be behind.
 */
export interface PosNotice {
  kind: "refused" | "failed";
  message: string;
  /** Present for a refusal, so a surface can key off the tag, not the prose. */
  reason?: Rejection;
  /** Distinguishes two identical messages in a row, so a toast re-announces. */
  id: number;
}

interface PosContextValue {
  // state mirrored from the server
  tables: Table[];
  guests: GuestProfile[];
  orders: Order[];
  activity: ActivityEvent[];

  hydrated: boolean;
  /** False while the event stream is down. */
  connected: boolean;
  /** The last thing that needs saying, or null. Render this — do not swallow it. */
  notice: PosNotice | null;
  dismissNotice: () => void;
  /**
   * How many writes are in flight. Anything that fires an irreversible action
   * disables itself while this is non-zero, because between the click and the
   * next revision the button is still enabled and a second tap sends a second
   * action with a different id — which server-side dedupe cannot catch.
   */
  pending: number;
  /** True when the reference data failed to load, so the UI can say so. */
  menuFailed: boolean;
  retryMenu: () => void;

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
  const [notice, setNotice] = useState<PosNotice | null>(null);
  const [pending, setPending] = useState(0);
  const [menuFailed, setMenuFailed] = useState(false);
  const [menuAttempt, setMenuAttempt] = useState(0);

  // Monotonic, so two identical messages in a row are still two notices and the
  // second one re-announces rather than looking like the first never cleared.
  const noticeId = useRef(0);
  const announce = useCallback(
    (next: Omit<PosNotice, "id">) => {
      noticeId.current += 1;
      setNotice({ ...next, id: noticeId.current });
    },
    [],
  );
  const dismissNotice = useCallback(() => setNotice(null), []);

  // Guards against an in-flight POST response landing after a newer SSE frame.
  const version = useRef(-1);

  const applyRevision = useCallback((next: Revision) => {
    // A version that went *backwards* means this is a different service, not a
    // stale frame — a redeployed or recreated database restarts at 0. Holding
    // the old high-water mark there would make the client ignore every future
    // revision while still showing itself as live.
    if (next.version < version.current) {
      version.current = next.version;
      setRevision(next);
      setHydrated(true);
      return;
    }
    if (next.version === version.current) return;
    version.current = next.version;
    setRevision(next);
    setHydrated(true);
  }, []);

  // Reference data changes rarely, but a single failed attempt used to leave
  // the app with an empty menu and no staff for the rest of the service, so
  // this backs off and keeps trying rather than giving up after one go.
  useEffect(() => {
    const controller = new AbortController();
    let timer: number | undefined;

    fetchMenu(controller.signal)
      .then((payload) => {
        setMenu(payload);
        setMenuFailed(false);
      })
      .catch((caught: unknown) => {
        if (controller.signal.aborted) return;
        setMenuFailed(true);
        announce({
          kind: "failed",
          message:
            caught instanceof Error
              ? caught.message
              : "Could not load the menu.",
        });
        // 2s, 4s, 8s... capped at 30s.
        const delay = Math.min(30_000, 2_000 * 2 ** Math.min(menuAttempt, 4));
        timer = window.setTimeout(() => setMenuAttempt((n) => n + 1), delay);
      });

    return () => {
      controller.abort();
      if (timer !== undefined) window.clearTimeout(timer);
    };
  }, [menuAttempt, announce]);

  const retryMenu = useCallback(() => setMenuAttempt((n) => n + 1), []);

  // The stream replays the current revision on connect, so this both hydrates
  // and keeps us live. No separate initial fetch is needed.
  useEffect(() => {
    return subscribeToState({
      onRevision: (next) => {
        applyRevision(next);
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
      setPending((n) => n + 1);
      postAction(action)
        .then((outcome) => {
          applyRevision(outcome);
          // The server answers 200 for a refusal — it is a normal outcome of a
          // busy floor, not a transport failure. Reading only the revision, as
          // this used to, meant a refused seating looked exactly like a click
          // that did nothing: no movement, no explanation, no way to tell the
          // difference from a dropped tap.
          if (outcome.outcome === "rejected") {
            announce({
              kind: "refused",
              reason: outcome.reason,
              message:
                outcome.reasonMessage ?? "That change was not allowed.",
            });
          }
        })
        .catch((caught: unknown) => {
          announce({
            kind: "failed",
            message:
              caught instanceof Error
                ? caught.message
                : "That change could not be saved.",
          });
        })
        .finally(() => setPending((n) => Math.max(0, n - 1)));
    },
    [applyRevision, announce],
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
      notice,
      dismissNotice,
      pending,
      menuFailed,
      retryMenu,
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
      notice,
      dismissNotice,
      pending,
      menuFailed,
      retryMenu,
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
