"use client";

import { useEffect, useRef, useState } from "react";
import { Mic, Square, Waves } from "lucide-react";
import type { RealtimeConnection } from "@elevenlabs/client";
import { apiUrl } from "@/lib/pos-client";

interface VoiceInputProps {
  label: string;
  value: string;
  onChange: (value: string) => void;
  placeholder: string;
  rows?: number;
  disabled?: boolean;
}

export function VoiceInput({
  label,
  value,
  onChange,
  placeholder,
  rows = 3,
  disabled = false,
}: VoiceInputProps) {
  const [recording, setRecording] = useState(false);
  const [connecting, setConnecting] = useState(false);
  const [status, setStatus] = useState("Type a note or use ElevenLabs voice");
  const connection = useRef<RealtimeConnection | null>(null);
  const baseText = useRef("");

  useEffect(() => () => connection.current?.close(), []);

  async function start() {
    if (connecting || recording || disabled) return;
    connection.current?.close();
    connection.current = null;
    setConnecting(true);
    setStatus("Connecting to ElevenLabs…");
    baseText.current = value.trim();

    try {
      const response = await fetch(apiUrl("/api/elevenlabs/token"), {
        cache: "no-store",
        credentials: "include",
      });
      const body = (await response.json()) as { token?: string; error?: string };
      if (!response.ok || !body.token) throw new Error(body.error ?? "Voice unavailable");

      const { Scribe, RealtimeEvents, CommitStrategy } = await import("@elevenlabs/client");
      const live = Scribe.connect({
        token: body.token,
        modelId: "scribe_v2_realtime",
        commitStrategy: CommitStrategy.VAD,
        vadSilenceThresholdSecs: 1.1,
        noVerbatim: true,
        keyterms: ["Ember", "tartare", "farro", "allergy", "gluten-free"],
        microphone: {
          echoCancellation: true,
          noiseSuppression: true,
          autoGainControl: true,
        },
      });

      live.on(RealtimeEvents.OPEN, () => {
        setConnecting(false);
        setRecording(true);
        setStatus("Listening — speak naturally");
      });
      live.on(RealtimeEvents.PARTIAL_TRANSCRIPT, (data) => {
        const next = [baseText.current, data.text].filter(Boolean).join(" ");
        onChange(next);
      });
      live.on(RealtimeEvents.COMMITTED_TRANSCRIPT, (data) => {
        baseText.current = [baseText.current, data.text].filter(Boolean).join(" ").trim();
        onChange(baseText.current);
      });
      live.on(RealtimeEvents.ERROR, (data) => {
        live.close();
        if (connection.current === live) connection.current = null;
        setConnecting(false);
        setStatus(data.error || "Voice connection failed. Continue typing.");
        setRecording(false);
      });
      live.on(RealtimeEvents.CLOSE, () => {
        if (connection.current === live) connection.current = null;
        setConnecting(false);
        setRecording(false);
        setStatus("Voice stopped — you can edit the transcript");
      });
      connection.current = live;
    } catch (error) {
      connection.current?.close();
      connection.current = null;
      setConnecting(false);
      setRecording(false);
      setStatus(error instanceof Error ? `${error.message} Continue typing.` : "Voice unavailable. Continue typing.");
    }
  }

  function stop() {
    connection.current?.close();
    connection.current = null;
    setRecording(false);
  }

  return (
    <div>
      <div className="mb-2 flex items-center justify-between gap-3">
        <label className="text-xs font-black uppercase tracking-[0.12em] text-ink-muted">
          {label}
        </label>
        <button
          type="button"
          onClick={recording ? stop : start}
          disabled={disabled || connecting}
          className={`flex items-center gap-2 rounded-full px-3 py-2 text-xs font-black ${
            recording
              ? "pulse-ring bg-accent text-white"
              : "border border-line bg-white hover:border-accent hover:text-accent disabled:cursor-not-allowed disabled:opacity-50"
          }`}
        >
          {recording ? <Square className="size-3 fill-current" /> : <Mic className="size-3.5" />}
          {recording ? "Stop" : connecting ? "Connecting…" : "Voice"}
        </button>
      </div>
      <textarea
        value={value}
        onChange={(event) => onChange(event.target.value)}
        placeholder={placeholder}
        rows={rows}
        disabled={disabled}
        className="w-full resize-none rounded-xl border border-line bg-white p-3 text-sm leading-6 outline-none placeholder:text-ink-muted/60 focus:border-accent focus:ring-2 focus:ring-accent/10 disabled:cursor-not-allowed disabled:border-line/60 disabled:bg-surface-muted disabled:text-ink-muted"
      />
      <p className="mt-1.5 flex items-center gap-1.5 text-[11px] text-ink-muted">
        <Waves className="size-3" />
        {status}
      </p>
    </div>
  );
}
