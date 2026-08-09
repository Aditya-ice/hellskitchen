import Link from "next/link";
import { ChefHat, LayoutDashboard, Radio } from "lucide-react";

export function SiteHeader({ active }: { active?: "pos" }) {
  return (
    <header className="border-b border-line bg-white/85 backdrop-blur">
      <div className="mx-auto flex h-16 max-w-[1440px] items-center justify-between px-4 sm:px-6">
        <Link href="/" className="flex items-center gap-3">
          <span className="grid size-9 place-items-center rounded-xl bg-navy text-white">
            <ChefHat className="size-5" />
          </span>
          <span>
            <span className="block text-sm font-black tracking-tight">EMBER POS</span>
            <span className="block text-[10px] font-bold uppercase tracking-[0.16em] text-ink-muted">
              Guest decisions
            </span>
          </span>
        </Link>

        <nav className="flex items-center gap-2 text-xs font-bold">
          <span className="hidden items-center gap-1.5 text-success sm:flex">
            <Radio className="size-3.5" />
            Dinner live
          </span>
          <Link
            href="/pos"
            className={`flex items-center gap-1.5 rounded-full border px-3 py-2 ${
              active === "pos"
                ? "border-navy bg-navy text-white"
                : "border-line bg-white text-ink-muted"
            }`}
          >
            <LayoutDashboard className="size-3.5" />
            Open POS
          </Link>
        </nav>
      </div>
    </header>
  );
}
