"use client";

import { useEffect, useRef, useState } from "react";
import { KeyRound, LoaderCircle, X } from "lucide-react";

import { setStaffPin } from "@/lib/pos-client";
import { usePos } from "@/components/pos-provider";

/**
 * Lets a manager issue a PIN to anyone on the roster.
 *
 * Without this the first-run bootstrap was the only way a PIN could ever be
 * set, so exactly one person could sign in, every action in the audit trail
 * carried their name, and a colleague who locked themselves out had no way
 * back onto the floor mid-service.
 */
export function StaffPins() {
  const pos = usePos();
  const [open, setOpen] = useState(false);
  const [staffId, setStaffId] = useState("");
  const [pin, setPin] = useState("");
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);

  const dialog = useRef<HTMLDivElement>(null);
  const opener = useRef<HTMLButtonElement>(null);

  // Escape closes, and focus returns to the button that opened it — neither of
  // which the app's existing dialogs do.
  useEffect(() => {
    if (!open) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    window.addEventListener("keydown", onKey);
    dialog.current?.querySelector("select")?.focus();

    // Captured now: by cleanup time the ref may point somewhere else.
    const trigger = opener.current;
    return () => {
      window.removeEventListener("keydown", onKey);
      trigger?.focus();
    };
  }, [open]);

  if (pos.identity?.role !== "manager") return null;

  const submit = async () => {
    if (!staffId || pin.length < 4 || busy) return;
    setBusy(true);
    setMessage(null);
    try {
      await setStaffPin(staffId, pin);
      const who = pos.staff.find((member) => member.id === staffId)?.name ?? staffId;
      setFailed(false);
      setMessage(`${who} can sign in with that PIN now.`);
      setPin("");
    } catch (caught) {
      setFailed(true);
      setMessage(caught instanceof Error ? caught.message : "That did not work.");
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <button
        ref={opener}
        type="button"
        onClick={() => setOpen(true)}
        className="flex items-center gap-2 rounded-full border border-line px-3 py-2 text-xs font-black hover:border-foreground"
      >
        <KeyRound className="size-3.5" aria-hidden="true" /> Staff PINs
      </button>

      {open ? (
        <div className="fixed inset-0 z-50 grid place-items-center p-4">
          <div
            className="absolute inset-0 bg-navy/40"
            onClick={() => setOpen(false)}
            aria-hidden="true"
          />
          <div
            ref={dialog}
            role="dialog"
            aria-modal="true"
            aria-labelledby="staff-pins-title"
            className="card relative w-full max-w-sm p-5"
          >
            <div className="flex items-start justify-between gap-4">
              <div>
                <h2 id="staff-pins-title" className="text-base font-black">
                  Staff PINs
                </h2>
                <p className="mt-1 text-xs text-ink-muted">
                  Setting a PIN signs that person out of any terminal they are
                  already on.
                </p>
              </div>
              <button
                type="button"
                onClick={() => setOpen(false)}
                aria-label="Close"
                className="grid size-11 shrink-0 place-items-center rounded-full text-ink-muted hover:text-foreground"
              >
                <X className="size-4" aria-hidden="true" />
              </button>
            </div>

            <form
              className="mt-4 space-y-3"
              onSubmit={(event) => {
                event.preventDefault();
                void submit();
              }}
            >
              <div>
                <label
                  htmlFor="pin-staff"
                  className="text-[11px] font-black uppercase tracking-wider text-ink-muted"
                >
                  Who
                </label>
                <select
                  id="pin-staff"
                  value={staffId}
                  onChange={(event) => setStaffId(event.target.value)}
                  className="mt-1 w-full rounded-xl border border-line bg-surface px-3 py-2.5 text-sm"
                >
                  <option value="">Choose someone…</option>
                  {pos.staff.map((member) => (
                    <option key={member.id} value={member.id}>
                      {member.name} · {member.role}
                    </option>
                  ))}
                </select>
              </div>

              <div>
                <label
                  htmlFor="pin-value"
                  className="text-[11px] font-black uppercase tracking-wider text-ink-muted"
                >
                  New PIN
                </label>
                <input
                  id="pin-value"
                  value={pin}
                  onChange={(event) =>
                    setPin(event.target.value.replace(/\D/g, "").slice(0, 12))
                  }
                  type="password"
                  inputMode="numeric"
                  className="mt-1 w-full rounded-xl border border-line bg-surface px-3 py-2.5 text-center text-lg tracking-[0.4em]"
                  placeholder="••••"
                />
                <p className="mt-1 text-[11px] text-ink-muted">4 to 12 digits.</p>
              </div>

              <p
                role="status"
                aria-live="polite"
                className={`min-h-[1.25rem] text-xs font-bold ${
                  failed ? "text-critical" : "text-success"
                }`}
              >
                {message}
              </p>

              <button
                type="submit"
                disabled={busy || !staffId || pin.length < 4}
                className="flex w-full items-center justify-center gap-2 rounded-xl bg-navy py-3 text-sm font-black text-white disabled:bg-line disabled:text-ink-muted"
              >
                {busy ? (
                  <LoaderCircle className="size-4 animate-spin" aria-hidden="true" />
                ) : null}
                Set PIN
              </button>
            </form>
          </div>
        </div>
      ) : null}
    </>
  );
}
