"use client";

import { useEffect, useState } from "react";
import { Check, CircleAlert, Clock3, Flame, ShieldAlert } from "lucide-react";
import { usePos } from "@/components/pos-provider";
import type { GuestProfile, Order } from "@/lib/domain";

/**
 * Back-of-house ticket rail, for a second screen above the pass.
 *
 * Shares the same live state as every other surface, so a ticket appears here
 * the instant a server fires it. The one thing the kitchen owns is bumping:
 * marking a ticket away clears it from the rail and from the menu-bar count.
 * Everything else about the floor is the host's to decide.
 */

/** Minutes since an order was fired. */
function ticketAge(order: Order, now: number): number {
  if (!order.sentAt) return 0;
  const sent = Date.parse(order.sentAt);
  if (Number.isNaN(sent)) return 0;
  return Math.max(0, Math.floor((now - sent) / 60000));
}

/**
 * Kitchen convention: a ticket goes from fine, to watch it, to it is late.
 * Thresholds are minutes since firing.
 */
function urgency(minutes: number): "fresh" | "working" | "late" {
  if (minutes >= 20) return "late";
  if (minutes >= 10) return "working";
  return "fresh";
}

const railStyle: Record<ReturnType<typeof urgency>, string> = {
  fresh: "border-line",
  working: "border-warning/60",
  late: "border-accent",
};

const ageStyle: Record<ReturnType<typeof urgency>, string> = {
  fresh: "text-ink-muted",
  working: "text-[#8a5b06]",
  late: "text-accent",
};

export function KitchenDisplay() {
  const pos = usePos();
  // Ticket age has to keep moving even when nothing on the floor changes.
  const [now, setNow] = useState(() => Date.now());

  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), 15_000);
    return () => window.clearInterval(timer);
  }, []);

  const tickets = pos.orders
    .filter((order) => order.status === "sent")
    .map((order) => ({
      order,
      guest: pos.guests.find((guest) => guest.id === order.guestId),
      table: pos.tables.find((table) => table.id === order.tableId),
      minutes: ticketAge(order, now),
    }))
    // Oldest first: the pass works the rail left to right.
    .sort((a, b) => b.minutes - a.minutes);

  if (!pos.hydrated) {
    return (
      <div className="grid min-h-screen place-items-center bg-navy text-white">
        <p className="text-sm font-black">Connecting to the pass…</p>
      </div>
    );
  }

  return (
    <main className="min-h-screen bg-navy p-6 text-white">
      <header className="mb-6 flex items-center justify-between">
        <div className="flex items-center gap-3">
          <span className="grid size-11 place-items-center rounded-2xl bg-white/10">
            <Flame className="size-6" />
          </span>
          <div>
            <h1 className="text-2xl font-black tracking-tight">Kitchen</h1>
            <p className="text-xs font-bold uppercase tracking-[0.16em] text-white/50">
              {pos.restaurant.serviceLabel}
            </p>
          </div>
        </div>
        <div className="flex items-center gap-6 text-right">
          <div>
            <p className="text-3xl font-black">{tickets.length}</p>
            <p className="text-[10px] font-bold uppercase tracking-[0.16em] text-white/50">
              Open tickets
            </p>
          </div>
          <span
            className={`flex items-center gap-2 rounded-full px-3 py-2 text-xs font-bold ${
              pos.connected ? "bg-[#71d8a0]/15 text-[#71d8a0]" : "bg-white/10 text-white/60"
            }`}
          >
            <span
              className={`size-2 rounded-full ${pos.connected ? "bg-[#71d8a0]" : "bg-white/40"}`}
            />
            {pos.connected ? "Live" : "Reconnecting"}
          </span>
        </div>
      </header>

      {tickets.length === 0 ? (
        <div className="grid min-h-[60vh] place-items-center rounded-3xl border border-white/10">
          <div className="text-center">
            <Clock3 className="mx-auto size-7 text-white/30" />
            <p className="mt-3 font-black">The pass is clear</p>
            <p className="mt-1 text-sm text-white/50">
              Tickets appear here the moment a server fires them.
            </p>
          </div>
        </div>
      ) : (
        <div className="grid gap-4 sm:grid-cols-2 xl:grid-cols-3">
          {tickets.map(({ order, guest, table, minutes }) => (
            <Ticket
              key={order.id}
              order={order}
              guest={guest}
              tableLabel={table?.label ?? "—"}
              minutes={minutes}
              nameFor={(id) =>
                pos.menuItems.find((item) => item.id === id)?.name ?? id
              }
              onBump={() => pos.completeOrder(order.id)}
              busy={pos.pending > 0}
            />
          ))}
        </div>
      )}
    </main>
  );
}

function Ticket({
  order,
  guest,
  tableLabel,
  minutes,
  nameFor,
  onBump,
  busy,
}: {
  order: Order;
  guest: GuestProfile | undefined;
  tableLabel: string;
  minutes: number;
  nameFor: (menuItemId: string) => string;
  onBump: () => void;
  /** A write is in flight; bumping again would clear an already-cleared ticket. */
  busy: boolean;
}) {
  const level = urgency(minutes);

  return (
    <article
      className={`rounded-3xl border-2 bg-white p-5 text-foreground ${railStyle[level]}`}
    >
      <div className="flex items-start justify-between gap-3">
        <div>
          <p className="text-lg font-black leading-tight">{tableLabel}</p>
          <p className="text-sm text-ink-muted">{guest?.name ?? "Walk-in"}</p>
        </div>
        <span className={`flex items-center gap-1.5 text-lg font-black ${ageStyle[level]}`}>
          <Clock3 className="size-4" />
          {minutes}m
        </span>
      </div>

      {guest?.allergies.length ? (
        <p className="mt-3 flex items-start gap-2 rounded-xl bg-accent/10 p-3 text-xs font-bold leading-5 text-accent">
          <ShieldAlert className="mt-0.5 size-4 shrink-0" />
          <span>
            Allergy: {guest.allergies.join(", ")}
            {guest.dietaryNeeds.length ? ` · ${guest.dietaryNeeds.join(", ")}` : ""}
          </span>
        </p>
      ) : guest?.dietaryNeeds.length ? (
        <p className="mt-3 rounded-xl bg-surface-muted p-3 text-xs font-bold text-ink-muted">
          {guest.dietaryNeeds.join(", ")}
        </p>
      ) : null}

      <ul className="mt-4 space-y-2">
        {order.lines.map((line) => (
          <li key={line.menuItemId} className="flex items-baseline gap-3 text-sm">
            <span className="min-w-6 font-black">{line.quantity}×</span>
            <span className="font-bold">{nameFor(line.menuItemId)}</span>
          </li>
        ))}
      </ul>

      {order.guestNotes ? (
        <p className="mt-4 flex items-start gap-2 border-t border-line pt-3 text-xs leading-5 text-ink-muted">
          <CircleAlert className="mt-0.5 size-3.5 shrink-0" />
          {order.guestNotes}
        </p>
      ) : null}

      <button
        type="button"
        onClick={onBump}
        // Bumping cannot be undone -- there is no un-bump action -- so this
        // stops the second of two quick taps clearing a ticket that the first
        // tap had already cleared.
        disabled={busy}
        className="mt-4 flex w-full items-center justify-center gap-2 rounded-xl bg-navy py-3 text-sm font-black text-white hover:bg-navy/90 disabled:bg-line disabled:text-ink-muted"
      >
        <Check className="size-4" aria-hidden="true" />
        Bump {tableLabel}
      </button>
    </article>
  );
}
