"use client";

import { useMemo, useState } from "react";
import {
  Accessibility,
  Armchair,
  ArrowRight,
  CalendarClock,
  Check,
  ChevronRight,
  CircleDollarSign,
  Clock3,
  History,
  LayoutGrid,
  Minus,
  Plus,
  ReceiptText,
  RotateCcw,
  Search,
  ShieldAlert,
  Sparkles,
  Star,
  UserRound,
  Users,
  UtensilsCrossed,
} from "lucide-react";
import { ingredients, menuItems, staff } from "@/data/demo";
import { orderTotal, recommendDishes, recommendTables } from "@/lib/decision-engine";
import type { GuestProfile, TableStatus } from "@/lib/domain";
import { usePos } from "@/components/pos-provider";

type Tab = "arrivals" | "floor" | "order" | "guest";

const tabs: { id: Tab; label: string; icon: typeof Users }[] = [
  { id: "arrivals", label: "Arrivals", icon: Users },
  { id: "floor", label: "Floor", icon: LayoutGrid },
  { id: "order", label: "Order", icon: UtensilsCrossed },
  { id: "guest", label: "Guest", icon: UserRound },
];

const statusStyle: Record<TableStatus, string> = {
  available: "border-success/30 bg-success/8 text-success",
  occupied: "border-navy bg-navy text-white",
  clearing: "border-warning/40 bg-warning/10 text-foreground",
  reserved: "border-line bg-surface-muted text-ink-muted",
};

function StatusPill({ status }: { status: GuestProfile["status"] }) {
  const classes =
    status === "waiting"
      ? "bg-warning/12 text-[#8a5b06]"
      : status === "seated" || status === "ordered"
        ? "bg-success/10 text-success"
        : "bg-surface-muted text-ink-muted";
  return (
    <span className={`rounded-full px-2.5 py-1 text-[10px] font-black uppercase tracking-wider ${classes}`}>
      {status}
    </span>
  );
}

export function PosShell() {
  const pos = usePos();
  const [activeTab, setActiveTab] = useState<Tab>("arrivals");
  const [search, setSearch] = useState("");
  const [walkInOpen, setWalkInOpen] = useState(false);
  const [walkInName, setWalkInName] = useState("");
  const [walkInSize, setWalkInSize] = useState(2);
  const [menuSection, setMenuSection] = useState<"all" | "starter" | "main" | "side" | "dessert">("all");

  const selectedGuest =
    pos.guests.find((guest) => guest.id === pos.selectedGuestId) ?? pos.guests[0];
  const selectedTable = pos.tables.find(
    (table) => table.seatedGuestId === selectedGuest?.id,
  );
  const selectedOrder = pos.orders.find((order) => order.guestId === selectedGuest?.id);
  const tableRecommendations = selectedGuest
    ? recommendTables(selectedGuest, pos.tables)
    : [];
  const dishRecommendations = selectedGuest ? recommendDishes(selectedGuest) : [];

  const filteredGuests = useMemo(
    () =>
      pos.guests.filter((guest) =>
        guest.name.toLowerCase().includes(search.toLowerCase()),
      ),
    [pos.guests, search],
  );

  const visibleMenu = menuItems.filter(
    (item) => menuSection === "all" || item.section === menuSection,
  );
  const openTables = pos.tables.filter((table) => table.status === "available").length;
  const waitingGuests = pos.guests.filter((guest) => guest.status === "waiting");

  function chooseGuest(id: string, tab?: Tab) {
    pos.selectGuest(id);
    if (tab) setActiveTab(tab);
  }

  function addWalkIn() {
    if (!walkInName.trim()) return;
    pos.addWalkIn(walkInName.trim(), walkInSize);
    setWalkInName("");
    setWalkInSize(2);
    setWalkInOpen(false);
  }

  return (
    <div className="min-h-[calc(100vh-4rem)]">
      <div className="border-b border-line bg-white">
        <div className="mx-auto flex max-w-[1440px] items-center justify-between gap-4 px-4 py-4 sm:px-6">
          <div>
            <p className="eyebrow text-accent">Sunday dinner · August 9</p>
            <h1 className="mt-1 text-xl font-black tracking-tight sm:text-2xl">
              Front-of-house workspace
            </h1>
          </div>
          <div className="hidden items-center gap-5 md:flex">
            <div>
              <p className="text-xl font-black">{waitingGuests.length}</p>
              <p className="text-[10px] font-bold uppercase tracking-wider text-ink-muted">Waiting</p>
            </div>
            <div>
              <p className="text-xl font-black">{openTables}</p>
              <p className="text-[10px] font-bold uppercase tracking-wider text-ink-muted">Open tables</p>
            </div>
            <div>
              <p className="text-xl font-black">12m</p>
              <p className="text-[10px] font-bold uppercase tracking-wider text-ink-muted">Avg wait</p>
            </div>
            <button
              type="button"
              onClick={pos.resetDemo}
              className="flex items-center gap-2 rounded-full border border-line px-3 py-2 text-xs font-black hover:border-foreground"
            >
              <RotateCcw className="size-3.5" /> Reset demo
            </button>
          </div>
        </div>
      </div>

      <div className="sticky top-0 z-20 border-b border-line bg-background/90 backdrop-blur">
        <div className="mx-auto flex max-w-[1440px] gap-1 overflow-x-auto px-3 py-2 sm:px-6">
          {tabs.map((tab) => {
            const Icon = tab.icon;
            return (
              <button
                key={tab.id}
                type="button"
                onClick={() => setActiveTab(tab.id)}
                className={`flex min-w-fit items-center gap-2 rounded-full px-4 py-2.5 text-xs font-black ${
                  activeTab === tab.id
                    ? "bg-navy text-white"
                    : "text-ink-muted hover:bg-white hover:text-foreground"
                }`}
              >
                <Icon className="size-4" />
                {tab.label}
              </button>
            );
          })}
        </div>
      </div>

      <main className="mx-auto max-w-[1440px] p-4 sm:p-6">
        {activeTab === "arrivals" && (
          <div className="grid gap-5 lg:grid-cols-[1.1fr_0.9fr]">
            <section className="card overflow-hidden">
              <div className="flex flex-col gap-3 border-b border-line p-4 sm:flex-row sm:items-center sm:justify-between">
                <div>
                  <h2 className="text-lg font-black">Arrivals</h2>
                  <p className="mt-1 text-xs text-ink-muted">Reservations and walk-ins for this service</p>
                </div>
                <div className="flex gap-2">
                  <label className="flex items-center gap-2 rounded-full border border-line bg-white px-3">
                    <Search className="size-3.5 text-ink-muted" />
                    <input
                      value={search}
                      onChange={(event) => setSearch(event.target.value)}
                      placeholder="Find guest"
                      className="w-28 bg-transparent py-2 text-xs outline-none"
                    />
                  </label>
                  <button
                    type="button"
                    onClick={() => setWalkInOpen((value) => !value)}
                    className="rounded-full bg-accent px-4 py-2 text-xs font-black text-white hover:bg-accent-dark"
                  >
                    + Walk-in
                  </button>
                </div>
              </div>

              {walkInOpen && (
                <div className="grid gap-3 border-b border-line bg-accent/5 p-4 sm:grid-cols-[1fr_auto_auto]">
                  <input
                    value={walkInName}
                    onChange={(event) => setWalkInName(event.target.value)}
                    placeholder="Guest name"
                    className="rounded-xl border border-line bg-white px-3 py-2.5 text-sm outline-none focus:border-accent"
                  />
                  <select
                    value={walkInSize}
                    onChange={(event) => setWalkInSize(Number(event.target.value))}
                    className="rounded-xl border border-line bg-white px-3 py-2.5 text-sm"
                  >
                    {[1, 2, 3, 4, 5, 6, 7, 8].map((size) => (
                      <option key={size} value={size}>Party of {size}</option>
                    ))}
                  </select>
                  <button type="button" onClick={addWalkIn} className="rounded-xl bg-navy px-4 py-2.5 text-sm font-black text-white">
                    Add guest
                  </button>
                </div>
              )}

              <div className="divide-y divide-line">
                {filteredGuests.map((guest) => {
                  const table = pos.tables.find((item) => item.seatedGuestId === guest.id);
                  return (
                    <button
                      key={guest.id}
                      type="button"
                      onClick={() => chooseGuest(guest.id)}
                      className={`grid w-full grid-cols-[1fr_auto] gap-3 p-4 text-left hover:bg-surface-muted/55 ${
                        guest.id === selectedGuest?.id ? "bg-accent/5" : ""
                      }`}
                    >
                      <div className="flex min-w-0 items-start gap-3">
                        <span className="grid size-10 shrink-0 place-items-center rounded-full bg-navy text-xs font-black text-white">
                          {guest.name.split(" ").map((part) => part[0]).join("")}
                        </span>
                        <div className="min-w-0">
                          <div className="flex flex-wrap items-center gap-2">
                            <p className="font-black">{guest.name}</p>
                            <StatusPill status={guest.status} />
                          </div>
                          <p className="mt-1 text-xs text-ink-muted">
                            Party of {guest.partySize} · {guest.reservationTime ?? "Walk-in"}
                            {table ? ` · ${table.label}` : ""}
                          </p>
                          <div className="mt-2 flex flex-wrap gap-1.5">
                            {[...guest.allergies.map((item) => `Allergy: ${item}`), ...guest.dietaryNeeds].map((tag) => (
                              <span key={tag} className="rounded-full bg-critical/8 px-2 py-1 text-[10px] font-bold text-critical">
                                {tag}
                              </span>
                            ))}
                            {guest.seatingPreferences.map((tag) => (
                              <span key={tag} className="rounded-full bg-surface-muted px-2 py-1 text-[10px] font-bold text-ink-muted">
                                {tag}
                              </span>
                            ))}
                          </div>
                        </div>
                      </div>
                      <div className="flex items-center gap-2">
                        {guest.status === "expected" && (
                          <span
                            role="button"
                            onClick={(event) => {
                              event.stopPropagation();
                              pos.checkInGuest(guest.id);
                            }}
                            className="rounded-full border border-line px-3 py-2 text-xs font-black hover:border-success hover:text-success"
                          >
                            Check in
                          </span>
                        )}
                        <ChevronRight className="size-4 text-ink-muted" />
                      </div>
                    </button>
                  );
                })}
              </div>
            </section>

            <aside className="space-y-4">
              {selectedGuest && (
                <>
                  <div className="card p-5">
                    <div className="flex items-start justify-between">
                      <div>
                        <p className="eyebrow text-accent">Selected party</p>
                        <h2 className="mt-2 text-2xl font-black">{selectedGuest.name}</h2>
                        <p className="mt-1 text-sm text-ink-muted">
                          Party of {selectedGuest.partySize} · {selectedGuest.visitCount} previous visits
                        </p>
                      </div>
                      <button
                        type="button"
                        onClick={() => setActiveTab("guest")}
                        className="rounded-full border border-line p-2 hover:border-foreground"
                        aria-label="Open guest profile"
                      >
                        <UserRound className="size-4" />
                      </button>
                    </div>
                    <p className="mt-4 rounded-xl bg-surface-muted p-3 text-sm leading-6 text-ink-muted">
                      {selectedGuest.notes || "No visit notes yet."}
                    </p>
                  </div>

                  {selectedGuest.status === "waiting" ? (
                    <div className="card overflow-hidden">
                      <div className="flex items-center justify-between border-b border-line p-4">
                        <div>
                          <p className="eyebrow text-success">AI table match</p>
                          <h3 className="mt-1 font-black">Best available options</h3>
                        </div>
                        <Sparkles className="size-5 text-warning" />
                      </div>
                      <div className="space-y-3 p-4">
                        {tableRecommendations.filter((item) => item.eligible).slice(0, 3).map((recommendation, index) => {
                          const table = pos.tables.find((item) => item.id === recommendation.id)!;
                          return (
                            <div key={table.id} className={`rounded-xl border p-3 ${index === 0 ? "border-success bg-success/5" : "border-line"}`}>
                              <div className="flex items-center justify-between">
                                <div className="flex items-center gap-3">
                                  <span className="grid size-10 place-items-center rounded-xl bg-navy text-xs font-black text-white">{table.label}</span>
                                  <div>
                                    <p className="text-sm font-black capitalize">{table.area} · {table.capacity} seats</p>
                                    <p className="mt-0.5 text-[11px] text-ink-muted">{recommendation.reasons.slice(0, 2).join(" · ")}</p>
                                  </div>
                                </div>
                                <span className="text-sm font-black text-success">{recommendation.score}%</span>
                              </div>
                              <button
                                type="button"
                                onClick={() => {
                                  pos.seatGuest(selectedGuest.id, table.id);
                                  setActiveTab("order");
                                }}
                                className="mt-3 flex w-full items-center justify-center gap-2 rounded-lg bg-navy py-2.5 text-xs font-black text-white hover:bg-accent"
                              >
                                Seat at {table.label} <ArrowRight className="size-3.5" />
                              </button>
                            </div>
                          );
                        })}
                      </div>
                    </div>
                  ) : (
                    <button
                      type="button"
                      onClick={() => setActiveTab("order")}
                      className="flex w-full items-center justify-between rounded-2xl bg-navy p-5 text-left text-white"
                    >
                      <span>
                        <span className="eyebrow text-white/50">Next step</span>
                        <span className="mt-1 block text-lg font-black">
                          {selectedGuest.status === "expected" ? "Check in this guest" : "Build their order"}
                        </span>
                      </span>
                      <ArrowRight className="size-5" />
                    </button>
                  )}
                </>
              )}
            </aside>
          </div>
        )}

        {activeTab === "floor" && (
          <div className="grid gap-5 lg:grid-cols-[1fr_340px]">
            <section className="card p-4 sm:p-6">
              <div className="flex flex-wrap items-center justify-between gap-3">
                <div>
                  <h2 className="text-lg font-black">Dining room</h2>
                  <p className="mt-1 text-xs text-ink-muted">Live table status and server balance</p>
                </div>
                <div className="flex flex-wrap gap-3 text-[10px] font-bold uppercase tracking-wider text-ink-muted">
                  <span className="flex items-center gap-1.5"><i className="size-2 rounded-full bg-success" /> Available</span>
                  <span className="flex items-center gap-1.5"><i className="size-2 rounded-full bg-navy" /> Occupied</span>
                  <span className="flex items-center gap-1.5"><i className="size-2 rounded-full bg-warning" /> Clearing</span>
                </div>
              </div>
              <div className="mt-6 grid grid-cols-2 gap-3 sm:grid-cols-3 xl:grid-cols-4">
                {pos.tables.map((table) => {
                  const guest = pos.guests.find((item) => item.id === table.seatedGuestId);
                  return (
                    <button
                      key={table.id}
                      type="button"
                      onClick={() => guest && chooseGuest(guest.id, "guest")}
                      className={`min-h-36 rounded-2xl border p-4 text-left ${statusStyle[table.status]}`}
                    >
                      <div className="flex items-start justify-between">
                        <span className="text-xl font-black">{table.label}</span>
                        <span className="flex items-center gap-1 text-[10px] font-bold uppercase tracking-wider">
                          <Users className="size-3" /> {table.capacity}
                        </span>
                      </div>
                      <p className="mt-5 text-xs font-black capitalize">{table.area}</p>
                      <p className="mt-1 text-[11px] opacity-70">
                        {guest ? `${guest.name} · ${guest.partySize} guests` : table.status === "clearing" ? `Ready in ${table.estimatedAvailableMinutes}m` : table.status}
                      </p>
                      {table.accessible && <Accessibility className="mt-3 size-4 opacity-60" />}
                    </button>
                  );
                })}
              </div>
            </section>

            <aside className="space-y-4">
              <div className="card p-5">
                <p className="eyebrow text-accent">Move or seat party</p>
                <h3 className="mt-2 text-lg font-black">{selectedGuest?.name ?? "Select a guest"}</h3>
                <p className="mt-1 text-xs text-ink-muted">
                  {selectedGuest ? `Party of ${selectedGuest.partySize}` : "Choose a party from Arrivals"}
                </p>
                <div className="mt-4 space-y-2">
                  {tableRecommendations.filter((item) => item.eligible).slice(0, 4).map((recommendation) => {
                    const table = pos.tables.find((item) => item.id === recommendation.id)!;
                    return (
                      <button
                        key={table.id}
                        type="button"
                        disabled={!selectedGuest}
                        onClick={() => selectedGuest && pos.seatGuest(selectedGuest.id, table.id)}
                        className="flex w-full items-center justify-between rounded-xl border border-line p-3 text-left hover:border-success disabled:opacity-40"
                      >
                        <span>
                          <span className="block text-sm font-black">{table.label} · <span className="capitalize">{table.area}</span></span>
                          <span className="mt-0.5 block text-[10px] text-ink-muted">{recommendation.reasons[0]}</span>
                        </span>
                        <span className="font-black text-success">{recommendation.score}</span>
                      </button>
                    );
                  })}
                </div>
              </div>
              <div className="card p-5">
                <p className="eyebrow text-ink-muted">Server sections</p>
                <div className="mt-4 space-y-3">
                  {staff.filter((member) => member.role === "server").map((server) => {
                    const count = pos.tables.filter((table) => table.serverId === server.id && table.status === "occupied").length;
                    return (
                      <div key={server.id} className="flex items-center justify-between">
                        <span className="flex items-center gap-2 text-sm font-bold">
                          <i className="grid size-8 place-items-center rounded-full bg-surface-muted text-[10px] not-italic">{server.initials}</i>
                          {server.name}
                        </span>
                        <span className="text-xs text-ink-muted">{count} active</span>
                      </div>
                    );
                  })}
                </div>
              </div>
            </aside>
          </div>
        )}

        {activeTab === "order" && (
          <div className="grid gap-5 xl:grid-cols-[280px_1fr_340px]">
            <aside className="card h-fit overflow-hidden">
              <div className="border-b border-line p-4">
                <p className="eyebrow text-accent">Ordering for</p>
                <h2 className="mt-2 text-xl font-black">{selectedGuest?.name ?? "No guest"}</h2>
                <p className="mt-1 text-xs text-ink-muted">
                  {selectedTable ? `${selectedTable.label} · Party of ${selectedGuest?.partySize}` : "Seat this party before ordering"}
                </p>
              </div>
              {selectedGuest && (
                <div className="p-4">
                  {(selectedGuest.allergies.length > 0 || selectedGuest.dietaryNeeds.length > 0) && (
                    <div className="rounded-xl border border-critical/20 bg-critical/5 p-3">
                      <p className="flex items-center gap-2 text-xs font-black text-critical">
                        <ShieldAlert className="size-4" /> Guest safety
                      </p>
                      <p className="mt-2 text-xs leading-5 text-ink-muted">
                        {[...selectedGuest.allergies.map((item) => `${item} allergy`), ...selectedGuest.dietaryNeeds].join(" · ")}
                      </p>
                      <p className="mt-2 text-[10px] font-bold text-critical">
                        Verify every allergy with kitchen staff.
                      </p>
                    </div>
                  )}
                  <div className="mt-4">
                    <p className="text-xs font-black">AI picks for this guest</p>
                    <div className="mt-2 space-y-2">
                      {dishRecommendations.filter((item) => item.eligible).slice(0, 3).map((recommendation, index) => {
                        const item = menuItems.find((menuItem) => menuItem.id === recommendation.id)!;
                        return (
                          <button
                            key={item.id}
                            type="button"
                            disabled={!selectedTable || selectedOrder?.status === "sent"}
                            onClick={() => pos.addOrderItem(selectedGuest.id, item.id)}
                            className="w-full rounded-xl border border-line p-3 text-left hover:border-accent disabled:opacity-50"
                          >
                            <div className="flex items-center justify-between">
                              <span className="text-xs font-black">{item.name}</span>
                              <span className={`text-[10px] font-black ${index === 0 ? "text-success" : "text-ink-muted"}`}>{recommendation.score}%</span>
                            </div>
                            <p className="mt-1 text-[10px] leading-4 text-ink-muted">{recommendation.reasons.slice(0, 2).join(" · ")}</p>
                            {recommendation.warnings.length > 0 && (
                              <p className="mt-1 text-[10px] font-bold text-warning">{recommendation.warnings[0]}</p>
                            )}
                          </button>
                        );
                      })}
                    </div>
                  </div>
                </div>
              )}
            </aside>

            <section className="card overflow-hidden">
              <div className="border-b border-line p-4">
                <div className="flex flex-wrap items-center justify-between gap-3">
                  <div>
                    <h2 className="text-lg font-black">Menu</h2>
                    <p className="mt-1 text-xs text-ink-muted">Availability and guest fit update live</p>
                  </div>
                  <div className="flex gap-1 rounded-full bg-surface-muted p-1">
                    {(["all", "starter", "main", "side", "dessert"] as const).map((section) => (
                      <button
                        key={section}
                        type="button"
                        onClick={() => setMenuSection(section)}
                        className={`rounded-full px-3 py-1.5 text-[10px] font-black capitalize ${menuSection === section ? "bg-white shadow-sm" : "text-ink-muted"}`}
                      >
                        {section}
                      </button>
                    ))}
                  </div>
                </div>
              </div>
              <div className="grid gap-3 p-4 sm:grid-cols-2">
                {visibleMenu.map((item) => {
                  const recommendation = dishRecommendations.find((entry) => entry.id === item.id);
                  const lowIngredients = ingredients.filter(
                    (ingredient) =>
                      item.ingredientIds.includes(ingredient.id) &&
                      ingredient.onHand / ingredient.par <= 0.25,
                  );
                  return (
                    <article key={item.id} className={`rounded-2xl border p-4 ${recommendation?.eligible ? "border-line bg-white" : "border-critical/20 bg-critical/3"}`}>
                      <div className="flex items-start justify-between gap-3">
                        <div>
                          <p className="font-black">{item.name}</p>
                          <p className="mt-1 text-xs leading-5 text-ink-muted">{item.description}</p>
                        </div>
                        <p className="font-black">${item.price}</p>
                      </div>
                      <div className="mt-3 flex flex-wrap gap-1.5">
                        {item.tags.map((tag) => (
                          <span key={tag} className="rounded-full bg-surface-muted px-2 py-1 text-[9px] font-bold text-ink-muted">{tag}</span>
                        ))}
                        {item.allergens.map((tag) => (
                          <span key={tag} className="rounded-full bg-warning/10 px-2 py-1 text-[9px] font-bold text-[#8a5b06]">{tag}</span>
                        ))}
                      </div>
                      {lowIngredients.length > 0 && (
                        <p className="mt-3 text-[10px] font-bold text-warning">
                          Low: {lowIngredients.map((item) => item.name).join(", ")}
                        </p>
                      )}
                      {recommendation && !recommendation.eligible && (
                        <p className="mt-3 text-[10px] font-bold text-critical">{recommendation.warnings[0]}</p>
                      )}
                      <div className="mt-4 flex items-center justify-between">
                        <span className="flex items-center gap-1 text-[10px] font-bold text-ink-muted">
                          <Clock3 className="size-3" /> {item.prepMinutes}m
                        </span>
                        <button
                          type="button"
                          disabled={!selectedGuest || !selectedTable || !recommendation?.eligible || selectedOrder?.status === "sent"}
                          onClick={() => selectedGuest && pos.addOrderItem(selectedGuest.id, item.id)}
                          className="grid size-8 place-items-center rounded-full bg-navy text-white hover:bg-accent disabled:bg-surface-muted disabled:text-ink-muted"
                          aria-label={`Add ${item.name}`}
                        >
                          <Plus className="size-4" />
                        </button>
                      </div>
                    </article>
                  );
                })}
              </div>
            </section>

            <aside className="card h-fit overflow-hidden">
              <div className="flex items-center justify-between border-b border-line p-4">
                <div>
                  <p className="eyebrow text-ink-muted">Current check</p>
                  <h2 className="mt-1 font-black">{selectedTable?.label ?? "No table"}</h2>
                </div>
                <ReceiptText className="size-5 text-accent" />
              </div>
              <div className="min-h-40 p-4">
                {!selectedOrder || selectedOrder.lines.length === 0 ? (
                  <div className="grid min-h-32 place-items-center text-center">
                    <div>
                      <UtensilsCrossed className="mx-auto size-6 text-line" />
                      <p className="mt-2 text-xs font-bold text-ink-muted">No items added yet</p>
                    </div>
                  </div>
                ) : (
                  <div className="space-y-3">
                    {selectedOrder.lines.map((line) => {
                      const item = menuItems.find((menuItem) => menuItem.id === line.menuItemId)!;
                      return (
                        <div key={line.menuItemId} className="flex items-start justify-between gap-3">
                          <div>
                            <p className="text-sm font-black">{item.name}</p>
                            <p className="mt-0.5 text-xs text-ink-muted">${item.price} each</p>
                          </div>
                          <div className="flex items-center gap-2">
                            <button type="button" onClick={() => selectedGuest && pos.removeOrderItem(selectedGuest.id, item.id)} className="grid size-6 place-items-center rounded-full border border-line"><Minus className="size-3" /></button>
                            <span className="w-4 text-center text-xs font-black">{line.quantity}</span>
                            <button type="button" onClick={() => selectedGuest && pos.addOrderItem(selectedGuest.id, item.id)} className="grid size-6 place-items-center rounded-full border border-line"><Plus className="size-3" /></button>
                          </div>
                        </div>
                      );
                    })}
                  </div>
                )}
                {selectedGuest && selectedOrder && (
                  <div className="mt-5 border-t border-line pt-4">
                    <label className="text-xs font-black uppercase tracking-[0.12em] text-ink-muted">
                      Order notes
                    </label>
                    <textarea
                      value={selectedOrder.guestNotes}
                      onChange={(event) =>
                        pos.updateOrderNotes(selectedGuest.id, event.target.value)
                      }
                      placeholder="e.g. Fire mains after starters, sauce on side…"
                      rows={2}
                      className="mt-2 w-full resize-none rounded-xl border border-line bg-white p-3 text-sm leading-6 outline-none focus:border-accent"
                    />
                  </div>
                )}
              </div>
              <div className="border-t border-line bg-surface-muted/60 p-4">
                <div className="flex items-center justify-between">
                  <span className="text-sm font-bold">Subtotal</span>
                  <span className="text-xl font-black">${orderTotal(selectedOrder).toFixed(2)}</span>
                </div>
                <button
                  type="button"
                  disabled={!selectedGuest || !selectedOrder?.lines.length || selectedOrder.status === "sent"}
                  onClick={() => selectedGuest && pos.sendOrder(selectedGuest.id)}
                  className="mt-4 flex w-full items-center justify-center gap-2 rounded-xl bg-accent py-3 text-sm font-black text-white hover:bg-accent-dark disabled:bg-line disabled:text-ink-muted"
                >
                  {selectedOrder?.status === "sent" ? <><Check className="size-4" /> Sent to kitchen</> : <>Send order <ArrowRight className="size-4" /></>}
                </button>
              </div>
            </aside>
          </div>
        )}

        {activeTab === "guest" && selectedGuest && (
          <div className="grid gap-5 lg:grid-cols-[0.8fr_1.2fr]">
            <section className="card overflow-hidden">
              <div className="bg-navy p-6 text-white">
                <div className="flex items-start justify-between">
                  <span className="grid size-14 place-items-center rounded-2xl bg-white/10 text-lg font-black">
                    {selectedGuest.name.split(" ").map((part) => part[0]).join("")}
                  </span>
                  <StatusPill status={selectedGuest.status} />
                </div>
                <h2 className="mt-5 text-3xl font-black">{selectedGuest.name}</h2>
                <p className="mt-1 text-sm text-white/55">Party of {selectedGuest.partySize} · {selectedGuest.visitCount} visits</p>
              </div>
              <div className="p-5">
                <div className="grid grid-cols-2 gap-3">
                  <div className="rounded-xl bg-surface-muted p-3">
                    <CalendarClock className="size-4 text-accent" />
                    <p className="mt-2 text-[10px] font-bold uppercase tracking-wider text-ink-muted">Reservation</p>
                    <p className="mt-1 text-sm font-black">{selectedGuest.reservationTime ?? "Walk-in"}</p>
                  </div>
                  <div className="rounded-xl bg-surface-muted p-3">
                    <History className="size-4 text-accent" />
                    <p className="mt-2 text-[10px] font-bold uppercase tracking-wider text-ink-muted">Last visit</p>
                    <p className="mt-1 text-sm font-black">{selectedGuest.lastVisit ?? "First visit"}</p>
                  </div>
                </div>
                <div className="mt-5">
                  <p className="eyebrow text-ink-muted">Preferences & safety</p>
                  <div className="mt-3 flex flex-wrap gap-2">
                    {selectedGuest.allergies.map((item) => <span key={item} className="rounded-full bg-critical/10 px-3 py-1.5 text-xs font-black text-critical">Allergy: {item}</span>)}
                    {selectedGuest.dietaryNeeds.map((item) => <span key={item} className="rounded-full bg-success/10 px-3 py-1.5 text-xs font-black text-success">{item}</span>)}
                    {selectedGuest.likes.map((item) => <span key={item} className="rounded-full bg-surface-muted px-3 py-1.5 text-xs font-bold">Likes {item}</span>)}
                  </div>
                </div>
              </div>
            </section>

            <div className="space-y-5">
              <section className="card p-5">
                <div className="flex items-center gap-2">
                  <Sparkles className="size-5 text-warning" />
                  <div>
                    <p className="eyebrow text-accent">Guest intelligence</p>
                    <h3 className="mt-1 text-lg font-black">Service notes</h3>
                  </div>
                </div>
                <div className="mt-5">
                  <label className="text-xs font-black uppercase tracking-[0.12em] text-ink-muted">
                    Notes for the team
                  </label>
                  <textarea
                    value={selectedGuest.notes}
                    onChange={(event) =>
                      pos.updateGuestNotes(selectedGuest.id, event.target.value)
                    }
                    placeholder="Capture the guest's seating needs, occasion, preferences, or pacing…"
                    rows={4}
                    className="mt-2 w-full resize-none rounded-xl border border-line bg-white p-3 text-sm leading-6 outline-none focus:border-accent"
                  />
                </div>
              </section>
              <section className="card p-5">
                <p className="eyebrow text-ink-muted">Decision summary</p>
                <div className="mt-4 grid gap-3 sm:grid-cols-3">
                  <div className="rounded-xl border border-line p-4">
                    <Armchair className="size-5 text-success" />
                    <p className="mt-3 text-sm font-black">{selectedTable?.label ?? tableRecommendations.find((item) => item.eligible)?.id.toUpperCase()}</p>
                    <p className="mt-1 text-xs text-ink-muted">{selectedTable ? "Current table" : "Best table match"}</p>
                  </div>
                  <div className="rounded-xl border border-line p-4">
                    <Star className="size-5 text-warning" />
                    <p className="mt-3 text-sm font-black">{menuItems.find((item) => item.id === dishRecommendations.find((entry) => entry.eligible)?.id)?.name}</p>
                    <p className="mt-1 text-xs text-ink-muted">Top dish match</p>
                  </div>
                  <div className="rounded-xl border border-line p-4">
                    <CircleDollarSign className="size-5 text-accent" />
                    <p className="mt-3 text-sm font-black">${orderTotal(selectedOrder).toFixed(2)}</p>
                    <p className="mt-1 text-xs text-ink-muted">Current subtotal</p>
                  </div>
                </div>
              </section>
              <section className="card p-5">
                <p className="eyebrow text-ink-muted">Recent floor activity</p>
                <div className="mt-4 space-y-3">
                  {pos.activity.length ? pos.activity.slice(0, 5).map((item) => (
                    <div key={item.id} className="flex gap-3 border-l-2 border-line pl-3">
                      <div>
                        <p className="text-xs font-black">{item.action}</p>
                        <p className="mt-1 text-xs text-ink-muted">{item.detail}</p>
                      </div>
                    </div>
                  )) : <p className="text-sm text-ink-muted">Actions from this service will appear here.</p>}
                </div>
              </section>
            </div>
          </div>
        )}
      </main>
    </div>
  );
}
