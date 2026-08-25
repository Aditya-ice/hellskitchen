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
import {
  createInitialPosState,
  demoGuests,
  type PosState,
  type Recommendation,
} from "@hellskitchen/shared";
import { apiFetch } from "@/lib/api-client";

interface PosContextValue extends PosState {
  hydrated: boolean;
  serverConnected: boolean;
  serverError: string | null;
  selectedGuestId: string | null;
  tableRecommendations: Recommendation[];
  dishRecommendations: Recommendation[];
  selectGuest: (id: string) => void;
  checkInGuest: (id: string) => Promise<void>;
  addWalkIn: (name: string, partySize: number) => Promise<string>;
  updateGuestNotes: (id: string, notes: string) => Promise<void>;
  seatGuest: (guestId: string, tableId: string) => Promise<void>;
  addOrderItem: (guestId: string, menuItemId: string) => Promise<void>;
  removeOrderItem: (guestId: string, menuItemId: string) => Promise<void>;
  updateOrderNotes: (guestId: string, notes: string) => Promise<void>;
  sendOrder: (guestId: string) => Promise<void>;
  resetDemo: () => Promise<void>;
  refreshState: () => Promise<void>;
}

const PosContext = createContext<PosContextValue | null>(null);

export function PosProvider({ children }: { children: React.ReactNode }) {
  const [state, setState] = useState<PosState>(createInitialPosState);
  const [selectedGuestId, setSelectedGuestId] = useState<string | null>(
    demoGuests[0]?.id ?? null,
  );
  const [hydrated, setHydrated] = useState(false);
  const [serverConnected, setServerConnected] = useState(false);
  const [serverError, setServerError] = useState<string | null>(null);
  const [tableRecommendations, setTableRecommendations] = useState<Recommendation[]>([]);
  const [dishRecommendations, setDishRecommendations] = useState<Recommendation[]>([]);
  const pollingRef = useRef<number | null>(null);

  const fetchLiveState = useCallback(async () => {
    try {
      const data = await apiFetch<PosState>("/v1/state");
      setState(data);
      setServerConnected(true);
      setServerError(null);
    } catch (err) {
      setServerConnected(false);
      setServerError(
        err instanceof Error ? err.message : "Unable to connect to POS server.",
      );
    } finally {
      setHydrated(true);
    }
  }, []);

  useEffect(() => {
    let mounted = true;

    async function init() {
      try {
        const data = await apiFetch<PosState>("/v1/state");
        if (mounted) {
          setState(data);
          setServerConnected(true);
          setServerError(null);
        }
      } catch (err) {
        if (mounted) {
          setServerConnected(false);
          setServerError(
            err instanceof Error ? err.message : "Unable to connect to POS server.",
          );
        }
      } finally {
        if (mounted) {
          setHydrated(true);
        }
      }
    }

    void init();

    pollingRef.current = window.setInterval(() => {
      void fetchLiveState();
    }, 4000);

    return () => {
      mounted = false;
      if (pollingRef.current) window.clearInterval(pollingRef.current);
    };
  }, [fetchLiveState]);

  const effectiveSelectedGuestId = state.guests.some(
    (guest) => guest.id === selectedGuestId,
  )
    ? selectedGuestId
    : state.guests[0]?.id ?? null;

  useEffect(() => {
    let mounted = true;
    if (effectiveSelectedGuestId && serverConnected) {
      void apiFetch<{
        tables: Recommendation[];
        dishes: Recommendation[];
      }>(`/v1/guests/${effectiveSelectedGuestId}/recommendations`)
        .then((data) => {
          if (mounted) {
            setTableRecommendations(data.tables);
            setDishRecommendations(data.dishes);
          }
        })
        .catch(() => {
          // Fall back silently if offline
        });
    }
    return () => {
      mounted = false;
    };
  }, [effectiveSelectedGuestId, serverConnected]);

  const selectGuest = useCallback((id: string) => {
    setSelectedGuestId(id);
  }, []);

  const checkInGuest = useCallback(
    async (guestId: string) => {
      setSelectedGuestId(guestId);
      try {
        const updated = await apiFetch<PosState>(`/v1/guests/${guestId}/check-in`, {
          method: "POST",
        });
        setState(updated);
      } catch (err) {
        setServerError(err instanceof Error ? err.message : "Failed to check in guest.");
      }
    },
    [],
  );

  const addWalkIn = useCallback(
    async (name: string, partySize: number) => {
      try {
        const res = await apiFetch<{ guest: { id: string }; state: PosState }>(
          "/v1/guests/walk-ins",
          {
            method: "POST",
            body: JSON.stringify({ name, partySize }),
          },
        );
        setState(res.state);
        setSelectedGuestId(res.guest.id);
        return res.guest.id;
      } catch (err) {
        setServerError(err instanceof Error ? err.message : "Failed to add walk-in.");
        return "";
      }
    },
    [],
  );

  const updateGuestNotes = useCallback(
    async (guestId: string, notes: string) => {
      try {
        const updated = await apiFetch<PosState>(`/v1/guests/${guestId}/notes`, {
          method: "PATCH",
          body: JSON.stringify({ notes }),
        });
        setState(updated);
      } catch (err) {
        setServerError(err instanceof Error ? err.message : "Failed to update notes.");
      }
    },
    [],
  );

  const seatGuest = useCallback(
    async (guestId: string, tableId: string) => {
      try {
        const updated = await apiFetch<PosState>(`/v1/guests/${guestId}/seat`, {
          method: "POST",
          body: JSON.stringify({ tableId }),
        });
        setState(updated);
      } catch (err) {
        setServerError(err instanceof Error ? err.message : "Failed to seat guest.");
      }
    },
    [],
  );

  const addOrderItem = useCallback(
    async (guestId: string, menuItemId: string) => {
      try {
        const updated = await apiFetch<PosState>(`/v1/orders/${guestId}/items`, {
          method: "POST",
          body: JSON.stringify({ menuItemId }),
        });
        setState(updated);
      } catch (err) {
        setServerError(err instanceof Error ? err.message : "Failed to add item.");
      }
    },
    [],
  );

  const removeOrderItem = useCallback(
    async (guestId: string, menuItemId: string) => {
      try {
        const updated = await apiFetch<PosState>(
          `/v1/orders/${guestId}/items/${menuItemId}`,
          {
            method: "DELETE",
          },
        );
        setState(updated);
      } catch (err) {
        setServerError(err instanceof Error ? err.message : "Failed to remove item.");
      }
    },
    [],
  );

  const updateOrderNotes = useCallback(
    async (guestId: string, notes: string) => {
      try {
        const updated = await apiFetch<PosState>(`/v1/orders/${guestId}/notes`, {
          method: "PATCH",
          body: JSON.stringify({ notes }),
        });
        setState(updated);
      } catch (err) {
        setServerError(err instanceof Error ? err.message : "Failed to update notes.");
      }
    },
    [],
  );

  const sendOrder = useCallback(
    async (guestId: string) => {
      try {
        const updated = await apiFetch<PosState>(`/v1/orders/${guestId}/send`, {
          method: "POST",
        });
        setState(updated);
      } catch (err) {
        setServerError(err instanceof Error ? err.message : "Failed to send order.");
      }
    },
    [],
  );

  const resetDemo = useCallback(async () => {
    setSelectedGuestId(demoGuests[0]?.id ?? null);
    try {
      const updated = await apiFetch<PosState>("/v1/demo/reset", {
        method: "POST",
      });
      setState(updated);
    } catch (err) {
      setServerError(err instanceof Error ? err.message : "Failed to reset demo.");
    }
  }, []);

  const value = useMemo<PosContextValue>(
    () => ({
      ...state,
      hydrated,
      serverConnected,
      serverError,
      selectedGuestId: effectiveSelectedGuestId,
      tableRecommendations,
      dishRecommendations,
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
      refreshState: fetchLiveState,
    }),
    [
      state,
      hydrated,
      serverConnected,
      serverError,
      effectiveSelectedGuestId,
      tableRecommendations,
      dishRecommendations,
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
      fetchLiveState,
    ],
  );

  return <PosContext.Provider value={value}>{children}</PosContext.Provider>;
}

export function usePos() {
  const context = useContext(PosContext);
  if (!context) throw new Error("usePos must be used inside PosProvider");
  return context;
}
