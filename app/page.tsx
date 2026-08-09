import Link from "next/link";
import {
  ArrowRight,
  Armchair,
  ChefHat,
  CircleCheck,
  Radio,
  ShieldCheck,
  Sparkles,
  UtensilsCrossed,
} from "lucide-react";

export default function Home() {
  return (
    <main className="min-h-screen overflow-hidden">
      <div className="mx-auto max-w-7xl px-5 pb-16 pt-6 sm:px-8 lg:pt-8">
        <nav className="flex items-center justify-between">
          <div className="flex items-center gap-3">
            <span className="grid size-11 place-items-center rounded-2xl bg-navy text-white">
              <ChefHat className="size-6" />
            </span>
            <div>
              <p className="font-black tracking-tight">EMBER POS</p>
              <p className="eyebrow text-ink-muted">Guest decisions</p>
            </div>
          </div>
          <span className="flex items-center gap-2 rounded-full border border-line bg-white px-3 py-2 text-xs font-bold text-success">
            <Radio className="size-3.5" />
            Dinner live
          </span>
        </nav>

        <section className="grid items-center gap-12 py-16 lg:grid-cols-[1.02fr_0.98fr] lg:py-24">
          <div>
            <p className="eyebrow mb-5 text-accent">AI-assisted front of house</p>
            <h1 className="max-w-3xl text-5xl font-black leading-[0.96] tracking-[-0.055em] sm:text-7xl">
              Seat smarter. Serve every guest personally.
            </h1>
            <p className="mt-7 max-w-xl text-lg leading-8 text-ink-muted">
              One workspace helps hosts choose the right table, remember what matters
              to each guest, and guide servers toward safe, available dishes—with
              every recommendation explained.
            </p>
            <div className="mt-9">
              <Link
                href="/pos"
                className="inline-flex items-center justify-center gap-2 rounded-full bg-accent px-6 py-3.5 text-sm font-black text-white hover:bg-accent-dark"
              >
                Open the live POS
                <ArrowRight className="size-4" />
              </Link>
            </div>
            <div className="mt-10 flex flex-wrap gap-x-7 gap-y-3 text-xs font-bold text-ink-muted">
              <span className="flex items-center gap-2">
                <CircleCheck className="size-4 text-success" /> Smart table matching
              </span>
              <span className="flex items-center gap-2">
                <CircleCheck className="size-4 text-success" /> Dietary-aware ordering
              </span>
              <span className="flex items-center gap-2">
                <CircleCheck className="size-4 text-success" /> Voice guest notes
              </span>
            </div>
          </div>

          <div className="relative mx-auto w-full max-w-xl">
            <div className="absolute -inset-8 -z-10 rounded-full bg-accent/10 blur-3xl" />
            <div className="card overflow-hidden p-3">
              <div className="rounded-2xl bg-navy p-5 text-white sm:p-7">
                <div className="flex items-start justify-between">
                  <div>
                    <p className="eyebrow text-white/50">Guest arriving</p>
                    <h2 className="mt-2 text-2xl font-black">Maya Chen · Party of 4</h2>
                    <p className="mt-2 text-sm text-white/55">Anniversary · window preferred · accessible</p>
                  </div>
                  <span className="rounded-full bg-warning px-3 py-1 text-[10px] font-black uppercase tracking-wider text-navy">
                    Waiting
                  </span>
                </div>
                <div className="mt-5 rounded-2xl bg-white/8 p-4">
                  <div className="flex items-center justify-between">
                    <div className="flex items-center gap-3">
                      <span className="grid size-11 place-items-center rounded-xl bg-white text-sm font-black text-navy">T2</span>
                      <div>
                        <p className="text-sm font-black">Best table match</p>
                        <p className="mt-1 text-xs text-white/55">Window · exact fit · accessible</p>
                      </div>
                    </div>
                    <span className="text-xl font-black text-[#71d8a0]">98%</span>
                  </div>
                </div>
              </div>
              <div className="grid gap-3 p-3 sm:grid-cols-3">
                <div className="rounded-2xl border border-line p-4">
                  <Armchair className="size-5 text-success" />
                  <p className="mt-3 text-sm font-black">Seat</p>
                  <p className="mt-1 text-[11px] leading-4 text-ink-muted">Capacity and flow balanced.</p>
                </div>
                <div className="rounded-2xl border border-line p-4">
                  <ShieldCheck className="size-5 text-accent" />
                  <p className="mt-3 text-sm font-black">Protect</p>
                  <p className="mt-1 text-[11px] leading-4 text-ink-muted">Allergies surfaced early.</p>
                </div>
                <div className="rounded-2xl border border-line p-4">
                  <Sparkles className="size-5 text-warning" />
                  <p className="mt-3 text-sm font-black">Recommend</p>
                  <p className="mt-1 text-[11px] leading-4 text-ink-muted">Dishes ranked with reasons.</p>
                </div>
              </div>
            </div>
          </div>
        </section>

        <section className="grid gap-4 border-t border-line pt-8 sm:grid-cols-3">
          {[
            [Armchair, "Arrival to table", "Match every party to the best table without losing sight of accessibility, wait time, or server load."],
            [UtensilsCrossed, "Table to order", "Turn guest history and live availability into useful, explainable dish suggestions."],
            [ShieldCheck, "Staff stays in control", "Recommendations assist the team. Allergy and service decisions always require human confirmation."],
          ].map(([Icon, title, copy]) => {
            const CardIcon = Icon as typeof Armchair;
            return (
              <div key={title as string} className="p-4">
                <CardIcon className="size-5 text-accent" />
                <h3 className="mt-4 font-black">{title as string}</h3>
                <p className="mt-2 text-sm leading-6 text-ink-muted">{copy as string}</p>
              </div>
            );
          })}
        </section>
      </div>
    </main>
  );
}
