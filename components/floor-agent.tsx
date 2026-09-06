"use client";

import { useRef, useState } from "react";
import { LoaderCircle, MessageCircleQuestion, Send, Sparkles, X } from "lucide-react";
import { askFloorAgent, type AgentAnswer } from "@/lib/pos-client";

/**
 * Natural-language questions about the service happening right now.
 *
 * The agent reads the same live floor everything else does and answers in
 * prose. It cannot change anything, which is why this is a panel and not a
 * command bar — there is no action to confirm, only an answer to read.
 */

/** Discoverability: nobody guesses what an agent can be asked. */
const EXAMPLES = [
  "Who has been waiting longest?",
  "What can I sell that uses up the carrots?",
  "What's on the pass right now?",
  "What should I offer table 2?",
];

export function FloorAgent() {
  const [open, setOpen] = useState(false);
  const [question, setQuestion] = useState("");
  const [answer, setAnswer] = useState<AgentAnswer | null>(null);
  const [asking, setAsking] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const inFlight = useRef<AbortController | null>(null);

  async function ask(text: string) {
    const trimmed = text.trim();
    if (!trimmed || asking) return;

    // A second question supersedes the first rather than racing it.
    inFlight.current?.abort();
    const controller = new AbortController();
    inFlight.current = controller;

    setAsking(true);
    setAnswer(null);
    setError(null);
    try {
      setAnswer(await askFloorAgent(trimmed, controller.signal));
    } catch (caught) {
      if (controller.signal.aborted) return;
      setError(
        caught instanceof Error ? caught.message : "The floor agent could not be reached.",
      );
    } finally {
      if (!controller.signal.aborted) setAsking(false);
    }
  }

  function close() {
    inFlight.current?.abort();
    setOpen(false);
    setAsking(false);
  }

  return (
    <>
      <button
        type="button"
        onClick={() => setOpen(true)}
        className="hidden items-center gap-2 rounded-full border border-line bg-white px-3 py-2 text-xs font-black hover:border-accent hover:text-accent sm:flex"
      >
        <MessageCircleQuestion className="size-3.5" />
        Ask the floor
      </button>

      {open && (
        <div className="fixed inset-0 z-50 flex items-end justify-end bg-navy/35 p-0 backdrop-blur-sm sm:p-4">
          <button
            type="button"
            aria-label="Close the floor agent"
            onClick={close}
            className="absolute inset-0 cursor-default"
          />
          <section className="relative z-10 flex h-[88vh] w-full flex-col overflow-hidden rounded-t-3xl bg-background shadow-2xl sm:h-auto sm:max-h-[90vh] sm:max-w-xl sm:rounded-3xl">
            <header className="flex items-center justify-between border-b border-line bg-white/95 p-5">
              <div className="flex items-center gap-3">
                <span className="grid size-10 place-items-center rounded-xl bg-navy text-white">
                  <Sparkles className="size-5" />
                </span>
                <div>
                  <p className="eyebrow text-accent">Reads the live floor</p>
                  <h2 className="mt-1 text-lg font-black">Ask the floor</h2>
                </div>
              </div>
              <button
                type="button"
                onClick={close}
                className="grid size-9 place-items-center rounded-full border border-line bg-white"
              >
                <X className="size-4" />
              </button>
            </header>

            <div className="flex-1 overflow-y-auto p-5">
              <form
                onSubmit={(event) => {
                  event.preventDefault();
                  void ask(question);
                }}
                className="flex gap-2"
              >
                <input
                  value={question}
                  onChange={(event) => setQuestion(event.target.value)}
                  placeholder="Ask about tonight's service…"
                  maxLength={2000}
                  autoFocus
                  className="min-w-0 flex-1 rounded-xl border border-line bg-white px-3 py-3 text-sm outline-none focus:border-accent focus:ring-2 focus:ring-accent/10"
                />
                <button
                  type="submit"
                  disabled={asking || !question.trim()}
                  className="flex items-center gap-2 rounded-xl bg-accent px-4 py-3 text-sm font-black text-white hover:bg-accent-dark disabled:bg-line disabled:text-ink-muted"
                >
                  {asking ? (
                    <LoaderCircle className="size-4 animate-spin" />
                  ) : (
                    <Send className="size-4" />
                  )}
                  Ask
                </button>
              </form>

              {!answer && !asking && !error && (
                <div className="mt-5">
                  <p className="text-xs font-black text-ink-muted">Try</p>
                  <div className="mt-2 flex flex-wrap gap-2">
                    {EXAMPLES.map((example) => (
                      <button
                        key={example}
                        type="button"
                        onClick={() => {
                          setQuestion(example);
                          void ask(example);
                        }}
                        className="rounded-full border border-line bg-white px-3 py-2 text-xs font-bold hover:border-accent hover:text-accent"
                      >
                        {example}
                      </button>
                    ))}
                  </div>
                </div>
              )}

              {asking && (
                <p className="mt-5 flex items-center gap-2 text-sm text-ink-muted">
                  <LoaderCircle className="size-4 animate-spin" />
                  Reading the floor…
                </p>
              )}

              {error && (
                <p className="mt-5 rounded-xl border border-critical/20 bg-critical/5 p-4 text-sm leading-6 text-critical">
                  {error}
                </p>
              )}

              {answer && (
                <div className="mt-5">
                  <p
                    className={`whitespace-pre-wrap rounded-2xl border p-4 text-sm leading-6 ${
                      answer.configured
                        ? "border-line bg-white"
                        : "border-warning/40 bg-warning/8"
                    }`}
                  >
                    {answer.answer}
                  </p>
                  {answer.toolsUsed.length > 0 && (
                    // Showing what it read makes the answer checkable rather
                    // than something to take on trust.
                    <p className="mt-2 text-[11px] text-ink-muted">
                      Read: {answer.toolsUsed.join(", ")}
                    </p>
                  )}
                  <p className="mt-3 text-[11px] leading-4 text-ink-muted">
                    The agent reports what the POS decided. It cannot change the
                    service, and allergy decisions always rest with the engine and
                    the kitchen.
                  </p>
                </div>
              )}
            </div>
          </section>
        </div>
      )}
    </>
  );
}
