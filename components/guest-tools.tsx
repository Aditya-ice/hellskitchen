"use client";

import { useState } from "react";
import {
  BookOpenText,
  ExternalLink,
  Hotel,
  LoaderCircle,
  MapPin,
  X,
} from "lucide-react";
import type { TavilyContext } from "@/lib/domain";
import { apiUrl } from "@/lib/pos-client";
import { usePos } from "@/components/pos-provider";

/** Shown when Tavily is unavailable; the kitchen is the source of truth. */
const FALLBACK_DISH_CONTEXT =
  "Seasonal preparation details are unavailable. Confirm ingredients and substitutions with the kitchen before describing them to a guest.";

type Tool = "dish" | "travel" | null;

export function GuestTools() {
  const { menuItems, restaurant } = usePos();
  const [tool, setTool] = useState<Tool>(null);
  const [dishId, setDishId] = useState<string>("");
  const [context, setContext] = useState<TavilyContext | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const aid = process.env.NEXT_PUBLIC_STAY22_AID ?? "";
  // The menu arrives from the server, so default to the first dish once it does.
  const activeDishId = dishId || menuItems[0]?.id || "";
  const venue = process.env.NEXT_PUBLIC_RESTAURANT_VENUE ?? restaurant.venue;
  const mapUrl = `https://www.stay22.com/embed/gm?aid=${encodeURIComponent(aid)}&address=${encodeURIComponent(venue)}&venue=${encodeURIComponent(restaurant.name)}`;

  async function researchDish() {
    const dish = menuItems.find((item) => item.id === activeDishId);
    if (!dish) return;
    setLoading(true);
    setContext(null);
    setError(null);
    try {
      const response = await fetch(apiUrl("/api/tavily/search"), {
        credentials: "include",
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ dishId: dish.id }),
      });
      const payload = (await response.json().catch(() => null)) as
        | Partial<TavilyContext> & { error?: string }
        | null;
      if (!response.ok) {
        throw new Error(payload?.error ?? "Dish context is temporarily unavailable.");
      }
      if (
        typeof payload?.isFallback !== "boolean" ||
        !Array.isArray(payload.sources) ||
        (payload.answer !== null && typeof payload.answer !== "string")
      ) {
        throw new Error("Dish context returned an invalid response.");
      }
      setContext(payload as TavilyContext);
    } catch (caught) {
      setError(
        caught instanceof Error
          ? caught.message
          : "Dish context is temporarily unavailable.",
      );
      setContext({
        answer: FALLBACK_DISH_CONTEXT,
        sources: [],
        isFallback: true,
      });
    } finally {
      setLoading(false);
    }
  }

  return (
    <>
      <div className="flex gap-2">
        <button
          type="button"
          onClick={() => setTool("dish")}
          className="hidden items-center gap-2 rounded-full border border-line bg-white px-3 py-2 text-xs font-black hover:border-accent hover:text-accent sm:flex"
        >
          <BookOpenText className="size-3.5" />
          Dish context
        </button>
        <button
          type="button"
          onClick={() => setTool("travel")}
          className="hidden items-center gap-2 rounded-full border border-line bg-white px-3 py-2 text-xs font-black hover:border-accent hover:text-accent sm:flex"
        >
          <Hotel className="size-3.5" />
          Guest concierge
        </button>
      </div>

      {tool && (
        <div className="fixed inset-0 z-50 flex items-end justify-end bg-navy/35 p-0 backdrop-blur-sm sm:p-4">
          <button
            type="button"
            aria-label="Close guest tool"
            onClick={() => setTool(null)}
            className="absolute inset-0 cursor-default"
          />
          <section className="relative z-10 h-[88vh] w-full overflow-y-auto rounded-t-3xl bg-background shadow-2xl sm:h-auto sm:max-h-[90vh] sm:max-w-xl sm:rounded-3xl">
            <header className="sticky top-0 z-10 flex items-center justify-between border-b border-line bg-white/95 p-5 backdrop-blur">
              <div className="flex items-center gap-3">
                <span className="grid size-10 place-items-center rounded-xl bg-navy text-white">
                  {tool === "dish" ? <BookOpenText className="size-5" /> : <Hotel className="size-5" />}
                </span>
                <div>
                  <p className="eyebrow text-accent">{tool === "dish" ? "Powered by Tavily" : "Powered by Stay22"}</p>
                  <h2 className="mt-1 text-lg font-black">{tool === "dish" ? "Guest-ready dish context" : "Nearby stays"}</h2>
                </div>
              </div>
              <button type="button" onClick={() => setTool(null)} className="grid size-9 place-items-center rounded-full border border-line bg-white">
                <X className="size-4" />
              </button>
            </header>

            {tool === "dish" ? (
              <div className="p-5">
                <p className="text-sm leading-6 text-ink-muted">
                  Pull current culinary background and sourcing context for the server.
                  Restaurant recipe data remains the source of truth.
                </p>
                <div className="mt-5 flex gap-2">
                  <select
                    value={activeDishId}
                    onChange={(event) => setDishId(event.target.value)}
                    disabled={!menuItems.length}
                    className="min-w-0 flex-1 rounded-xl border border-line bg-white px-3 py-3 text-sm font-bold"
                  >
                    {menuItems.map((item) => <option key={item.id} value={item.id}>{item.name}</option>)}
                  </select>
                  <button
                    type="button"
                    onClick={researchDish}
                    disabled={loading}
                    className="rounded-xl bg-accent px-4 py-3 text-sm font-black text-white disabled:opacity-60"
                  >
                    {loading ? <LoaderCircle className="size-4 animate-spin" /> : "Research"}
                  </button>
                </div>
                {error && (
                  <p
                    role="alert"
                    className="mt-4 rounded-xl bg-warning/10 p-3 text-xs font-bold text-[#76510c]"
                  >
                    {error} Showing seeded guidance instead.
                  </p>
                )}
                {context && (
                  <div className="mt-5">
                    <div className="rounded-2xl border border-line bg-white p-4">
                      <div className="flex items-center justify-between gap-3">
                        <p className="eyebrow text-ink-muted">{context.isFallback ? "Seeded fallback" : "Current web context"}</p>
                        {!context.isFallback && <span className="rounded-full bg-success/10 px-2 py-1 text-[9px] font-black uppercase tracking-wider text-success">Live</span>}
                      </div>
                      <p className="mt-3 text-sm leading-6">{context.answer}</p>
                    </div>
                    {context.sources.length > 0 && (
                      <div className="mt-4 space-y-2">
                        <p className="eyebrow text-ink-muted">Sources</p>
                        {context.sources.map((source) => (
                          <a
                            key={source.url}
                            href={source.url}
                            target="_blank"
                            rel="noreferrer"
                            className="flex items-center justify-between rounded-xl border border-line bg-white p-3 text-xs font-bold hover:border-accent"
                          >
                            <span className="line-clamp-1">{source.title}</span>
                            <ExternalLink className="ml-3 size-3.5 shrink-0" />
                          </a>
                        ))}
                      </div>
                    )}
                    <p className="mt-4 rounded-xl bg-warning/10 p-3 text-[11px] font-bold leading-5 text-[#76510c]">
                      Do not use web context for allergy clearance. Confirm ingredients and cross-contact with the kitchen.
                    </p>
                  </div>
                )}
              </div>
            ) : (
              <div className="p-5">
                <div className="mb-4 flex items-center gap-2 text-sm font-bold">
                  <MapPin className="size-4 text-accent" />
                  {venue}
                </div>
                {aid ? (
                  <iframe
                    title="Stay22 nearby accommodation map"
                    src={mapUrl}
                    className="h-[460px] w-full rounded-2xl border border-line bg-white"
                    loading="lazy"
                  />
                ) : (
                  <div className="grid h-72 place-items-center rounded-2xl border border-dashed border-line bg-white p-8 text-center">
                    <div>
                      <Hotel className="mx-auto size-8 text-accent" />
                      <p className="mt-4 font-black">Stay22 map ready</p>
                      <p className="mt-2 text-sm leading-6 text-ink-muted">
                        Add your Stay22 affiliate ID to show live nearby hotels for visiting guests.
                      </p>
                    </div>
                  </div>
                )}
                <p className="mt-4 text-xs leading-5 text-ink-muted">
                  A lightweight concierge option for guests traveling to a dinner, event, or celebration near the restaurant.
                </p>
              </div>
            )}
          </section>
        </div>
      )}
    </>
  );
}
