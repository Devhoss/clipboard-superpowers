import { useCallback, useEffect, useState } from "react";
import { api } from "@/lib/api";
import type { AppSettings, HistoryStats } from "@/lib/types";
import { cn } from "@/lib/utils";

function Toggle({
  checked,
  onChange,
  label,
  hint,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
  label: string;
  hint?: string;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      onClick={() => onChange(!checked)}
      className="flex w-full items-center justify-between gap-3 py-1.5 text-left"
    >
      <span className="min-w-0">
        <span className="block text-[13px] text-foreground">{label}</span>
        {hint && <span className="block text-[11px] text-muted-foreground">{hint}</span>}
      </span>
      <span
        aria-hidden="true"
        className={cn(
          "relative h-5 w-9 shrink-0 rounded-full transition-colors duration-200",
          checked ? "bg-green-500" : "bg-white/15",
        )}
      >
        <span
          className={cn(
            "absolute top-0.5 size-4 rounded-full bg-white shadow transition-all duration-200",
            checked ? "left-[18px]" : "left-0.5",
          )}
        />
      </span>
    </button>
  );
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="rounded-xl border border-border/80 bg-card/85 p-3">
      <h2 className="mb-1 text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
        {title}
      </h2>
      {children}
    </section>
  );
}

function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(2)} MB`;
}

/** e.code-based so Shift+2 records "2", not "@". Returns null for unsupported. */
function keyEventToCombo(e: KeyboardEvent): string | null {
  const mods: string[] = [];
  if (e.ctrlKey) mods.push("Ctrl");
  if (e.altKey) mods.push("Alt");
  if (e.shiftKey) mods.push("Shift");
  if (e.metaKey) mods.push("Super");
  if (mods.length === 0) return null;
  const m =
    /^Key([A-Z])$/.exec(e.code) ?? /^Digit([0-9])$/.exec(e.code) ?? /^(F\d{1,2})$/.exec(e.code);
  if (!m) return null;
  return [...mods, m[1]].join("+");
}

export function SettingsPanel({
  initial,
  onSaved,
  onCleared,
}: {
  initial: AppSettings;
  onSaved: (s: AppSettings) => void;
  onCleared: () => void;
}) {
  const [draft, setDraft] = useState<AppSettings>(initial);
  const [recording, setRecording] = useState(false);
  const [stats, setStats] = useState<HistoryStats | null>(null);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [armed, setArmed] = useState<"unpinned" | "all" | null>(null);

  const refreshStats = useCallback(() => {
    api.getStats().then(setStats).catch(console.error);
  }, []);

  useEffect(() => {
    refreshStats();
  }, [refreshStats]);

  useEffect(() => {
    if (!recording) return;
    const onKey = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      const combo = keyEventToCombo(e);
      if (!combo) return; // modifier-only or unsupported — keep listening
      setDraft((d) => ({ ...d, hotkey: combo }));
      setRecording(false);
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [recording]);

  const dirty = JSON.stringify(draft) !== JSON.stringify(initial);

  const save = () => {
    setError("");
    setNotice("");
    api
      .updateSettings(draft)
      .then((saved) => {
        onSaved(saved);
        setNotice("Saved — hotkey, autostart and limits applied live.");
      })
      .catch((e) => setError(String(e)));
  };

  const clear = (mode: "unpinned" | "all") => {
    if (armed !== mode) {
      setArmed(mode);
      return;
    }
    setArmed(null);
    setError("");
    api
      .clearHistory(mode === "all")
      .then((n) => {
        setNotice(`Cleared ${n} item${n === 1 ? "" : "s"}.`);
        refreshStats();
        onCleared();
      })
      .catch((e) => setError(String(e)));
  };

  return (
    <div className="flex min-w-0 flex-1 flex-col gap-2.5 overflow-y-auto pr-1 pb-2">
      <Section title="Popup">
        <button
          type="button"
          onClick={() => setRecording(true)}
          className="flex w-full items-center justify-between gap-3 py-1.5 text-left"
        >
          <span className="min-w-0">
            <span className="block text-[13px] text-foreground">Toggle hotkey</span>
            <span className="block text-[11px] text-muted-foreground">
              {recording ? "Press keys now…" : "Click to record a new combo"}
            </span>
          </span>
          <kbd
            className={cn(
              "shrink-0 rounded-md border border-border bg-muted px-2 py-1 font-mono text-[11px] text-foreground",
              recording && "animate-pulse border-primary",
            )}
          >
            {draft.hotkey}
          </kbd>
        </button>
        <Toggle
          checked={draft.hide_on_blur}
          onChange={(v) => setDraft((d) => ({ ...d, hide_on_blur: v }))}
          label="Hide when focus is lost"
          hint="Off keeps the popup open until Esc or the hotkey"
        />
        <Toggle
          checked={draft.launch_on_login}
          onChange={(v) => setDraft((d) => ({ ...d, launch_on_login: v }))}
          label="Launch on login"
        />
      </Section>

      <Section title="Capture">
        <Toggle
          checked={draft.capture_text}
          onChange={(v) => setDraft((d) => ({ ...d, capture_text: v }))}
          label="Capture text"
        />
        <Toggle
          checked={draft.capture_images}
          onChange={(v) => setDraft((d) => ({ ...d, capture_images: v }))}
          label="Capture images"
          hint="Screenshots and copied pictures"
        />
      </Section>

      <Section title="History">
        <label className="flex w-full items-center justify-between gap-3 py-1.5">
          <span className="min-w-0">
            <span className="block text-[13px] text-foreground">Keep at most</span>
            <span className="block text-[11px] text-muted-foreground">
              Unpinned items, 100–5000. Pinned are never pruned.
            </span>
          </span>
          <input
            type="number"
            min={100}
            max={5000}
            step={100}
            value={draft.max_items}
            onChange={(e) =>
              setDraft((d) => ({ ...d, max_items: Number(e.target.value) || 100 }))
            }
            className="w-20 shrink-0 rounded-md border border-input bg-transparent px-2 py-1 text-right text-[13px] text-foreground outline-none focus:border-ring"
          />
        </label>
        <div className="mt-1 flex gap-2">
          <button
            type="button"
            onClick={() => clear("unpinned")}
            className="flex-1 rounded-lg border border-border/70 bg-muted/60 px-2 py-1.5 text-xs text-muted-foreground transition hover:bg-muted hover:text-foreground"
          >
            {armed === "unpinned" ? "Tap again to confirm" : "Clear unpinned"}
          </button>
          <button
            type="button"
            onClick={() => clear("all")}
            className="flex-1 rounded-lg border border-destructive/40 bg-destructive/10 px-2 py-1.5 text-xs text-destructive transition hover:bg-destructive/20"
          >
            {armed === "all" ? "Tap again to confirm" : "Clear everything"}
          </button>
        </div>
        {stats && (
          <p className="mt-2 text-[11px] text-muted-foreground">
            {stats.total} items ({stats.pinned} pinned) · DB {formatBytes(stats.db_bytes)}
          </p>
        )}
      </Section>

      {error && <p className="text-xs text-destructive">{error}</p>}
      {notice && !error && <p className="text-xs text-muted-foreground">{notice}</p>}

      <button
        type="button"
        onClick={save}
        disabled={!dirty}
        className="shrink-0 rounded-lg bg-primary px-3 py-2 text-sm font-medium text-primary-foreground transition disabled:cursor-not-allowed disabled:opacity-40"
      >
        Save settings
      </button>
    </div>
  );
}
