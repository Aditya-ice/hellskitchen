"use client";

import { useEffect, useRef, useState } from "react";
import { Delete, LoaderCircle, LockKeyhole } from "lucide-react";

import {
  fetchIdentity,
  login,
  setupFirstManager,
  type AuthState,
} from "@/lib/pos-client";

const KEYS = ["1", "2", "3", "4", "5", "6", "7", "8", "9", "", "0", "erase"];
const MIN_PIN = 4;
const MAX_PIN = 12;

/** Remembers which screen this is, so the audit trail can name it. */
const TERMINAL_KEY = "ember.terminalId";

function terminalId(): string {
  try {
    const stored = window.localStorage.getItem(TERMINAL_KEY);
    if (stored) return stored;
  } catch {
    // Private windows and locked-down browsers throw on access.
  }
  // Not an identity claim — the server records it beside the staff id so two
  // screens can be told apart, nothing more.
  const generated = `terminal-${Math.random().toString(36).slice(2, 8)}`;
  try {
    window.localStorage.setItem(TERMINAL_KEY, generated);
  } catch {
    // Fine: a terminal that cannot remember its name gets a new one each time.
  }
  return generated;
}

interface SignInProps {
  /** Called once the terminal is signed in. */
  onSignedIn: () => void;
  /** Shown above the keypad when a session ended rather than never existing. */
  reason?: string;
}

/**
 * PIN entry for a shared terminal.
 *
 * A restaurant terminal is used by several people across a service, so the
 * thing being authenticated is a person at a screen, not a browser profile.
 * That is why this is a keypad and not an email and password form: it has to be
 * usable in three seconds with one hand between tables.
 */
export function SignIn({ onSignedIn, reason }: SignInProps) {
  const [auth, setAuth] = useState<AuthState | null>(null);
  const [staffId, setStaffId] = useState("");
  const [pin, setPin] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const liveRegion = useRef<HTMLParagraphElement>(null);

  useEffect(() => {
    const controller = new AbortController();
    fetchIdentity(controller.signal)
      .then((state) => {
        setAuth(state);
        if (state.authenticated) onSignedIn();
      })
      .catch(() => {
        if (!controller.signal.aborted) {
          setAuth({ authenticated: false });
          setError("Could not reach the service. Check the connection.");
        }
      });
    return () => controller.abort();
  }, [onSignedIn]);

  const needsSetup = auth?.needsSetup === true;

  const submit = async () => {
    if (pin.length < MIN_PIN || !staffId.trim() || busy) return;
    setBusy(true);
    setError(null);
    try {
      if (needsSetup) {
        await setupFirstManager(staffId.trim(), pin);
      }
      await login(staffId.trim(), pin, terminalId());
      onSignedIn();
    } catch (caught) {
      setPin("");
      setError(
        caught instanceof Error ? caught.message : "That did not work.",
      );
    } finally {
      setBusy(false);
    }
  };

  if (auth === null) {
    return (
      <main className="grid min-h-screen place-items-center p-6">
        <p className="flex items-center gap-2 text-sm text-ink-muted">
          <LoaderCircle className="size-4 animate-spin" aria-hidden="true" />
          Checking this terminal…
        </p>
      </main>
    );
  }

  return (
    <main className="grid min-h-screen place-items-center p-4">
      <div className="card w-full max-w-sm p-6">
        <div className="flex items-center gap-2">
          <LockKeyhole className="size-4 text-accent" aria-hidden="true" />
          <p className="eyebrow text-accent">
            {needsSetup ? "First run" : "Staff sign-in"}
          </p>
        </div>
        <h1 className="mt-2 text-xl font-black tracking-tight">
          {needsSetup ? "Set the first manager PIN" : "Sign in to the floor"}
        </h1>
        <p className="mt-1.5 text-sm text-ink-muted">
          {needsSetup
            ? "Nobody has a PIN on this terminal yet. The first one has to be a manager's — they can add everyone else."
            : "Every action on the floor is recorded against whoever is signed in."}
        </p>

        {reason ? (
          <p className="mt-3 rounded-xl bg-warning/12 px-3 py-2 text-xs font-bold text-[#8a5b06]">
            {reason}
          </p>
        ) : null}

        <form
          className="mt-5 space-y-3"
          onSubmit={(event) => {
            event.preventDefault();
            void submit();
          }}
        >
          <div>
            <label
              htmlFor="staff-id"
              className="text-[11px] font-black uppercase tracking-wider text-ink-muted"
            >
              Staff ID
            </label>
            <input
              id="staff-id"
              value={staffId}
              onChange={(event) => setStaffId(event.target.value)}
              autoComplete="username"
              className="mt-1 w-full rounded-xl border border-line bg-surface px-3 py-2.5 text-sm"
              placeholder="manager-1"
            />
          </div>

          <div>
            <label
              htmlFor="pin"
              className="text-[11px] font-black uppercase tracking-wider text-ink-muted"
            >
              PIN
            </label>
            <input
              id="pin"
              value={pin}
              onChange={(event) =>
                setPin(
                  event.target.value.replace(/\D/g, "").slice(0, MAX_PIN),
                )
              }
              type="password"
              inputMode="numeric"
              autoComplete="current-password"
              className="mt-1 w-full rounded-xl border border-line bg-surface px-3 py-2.5 text-center text-lg tracking-[0.4em]"
              placeholder="••••"
            />
          </div>

          {/* A keypad as well as the field: this is a touch terminal, and the
              on-screen keyboard covers half the screen on a tablet. */}
          <div className="grid grid-cols-3 gap-2">
            {KEYS.map((key, index) =>
              key === "" ? (
                <span key={index} />
              ) : (
                <button
                  key={index}
                  type="button"
                  onClick={() =>
                    setPin((current) =>
                      key === "erase"
                        ? current.slice(0, -1)
                        : (current + key).slice(0, MAX_PIN),
                    )
                  }
                  aria-label={key === "erase" ? "Delete last digit" : key}
                  className="grid h-12 place-items-center rounded-xl border border-line bg-surface text-base font-black hover:border-foreground"
                >
                  {key === "erase" ? (
                    <Delete className="size-4" aria-hidden="true" />
                  ) : (
                    key
                  )}
                </button>
              ),
            )}
          </div>

          <p
            ref={liveRegion}
            role="alert"
            aria-live="assertive"
            className="min-h-[1.25rem] text-xs font-bold text-critical"
          >
            {error}
          </p>

          <button
            type="submit"
            disabled={busy || pin.length < MIN_PIN || !staffId.trim()}
            className="flex w-full items-center justify-center gap-2 rounded-xl bg-accent py-3 text-sm font-black text-white hover:bg-accent-dark disabled:bg-line disabled:text-ink-muted"
          >
            {busy ? (
              <LoaderCircle className="size-4 animate-spin" aria-hidden="true" />
            ) : null}
            {needsSetup ? "Set PIN and sign in" : "Sign in"}
          </button>
        </form>
      </div>
    </main>
  );
}
