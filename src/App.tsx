import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { openPath, revealItemInDir } from "@tauri-apps/plugin-opener";
import { DetailPane } from "./components/DetailPane";
import { EntryList } from "./components/EntryList";
import { SettingsPanel } from "./components/SettingsPanel";
import { TypeDropdown } from "./components/TypeDropdown";
import { api, EVENTS } from "./lib/api";
import { matchesCombo, moveSelection } from "./lib/keyboardNav";
import { evictCachedImage } from "@/lib/imageCache";
import type { AppSettings, Category, ClipboardItem, ThemeMode } from "./lib/types";
import {
  ArrowLeft,
  Clipboard as ClipboardIcon,
  Copy,
  Keyboard,
  Laptop,
  Moon,
  Search as SearchIcon,
  Settings as SettingsIcon,
  Sun,
  X,
} from "lucide-react";
import { cn } from "@/lib/utils";
import { openUrl } from "@tauri-apps/plugin-opener";

// Keep the renderer usable while it is being previewed in a regular browser.
// Tauri injects this bridge before the app loads, but it is intentionally absent
// from Vite's browser preview. Creating a window handle without it throws before
// React gets a chance to render anything.
const appWindow =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window
    ? getCurrentWebviewWindow()
    : null;

const THEME_STORAGE_KEY = "clipboard-superpowers-theme";

/** Pinned cards always float above history, newest first within each group.
// Backend fetch already orders this way, but optimistic local moves (click
// to copy, live captures) prepended blindly and stranded pinned cards below
// fresh items — every local mutation goes through placeItem instead. */
function byPinnedThenRecent(a: ClipboardItem, b: ClipboardItem): number {
  if (a.pinned !== b.pinned) return a.pinned ? -1 : 1;
  return b.created_at.localeCompare(a.created_at);
}

function placeItem(prev: ClipboardItem[], item: ClipboardItem): ClipboardItem[] {
  return [item, ...prev.filter((i) => i.id !== item.id)].sort(byPinnedThenRecent);
}

function getInitialTheme(): ThemeMode {
  const stored = localStorage.getItem(THEME_STORAGE_KEY);
  return stored === "light" || stored === "dark" || stored === "system" ? stored : "system";
}

function App() {
  const [items, setItems] = useState<ClipboardItem[]>([]);
  const [search, setSearch] = useState("");
  const [debounced, setDebounced] = useState("");
  const [category, setCategory] = useState<Category | "all">("all");
  const [theme, setTheme] = useState<ThemeMode>(getInitialTheme);
  const [visibleCount, setVisibleCount] = useState(50);
  const [view, setView] = useState<"list" | "settings">("list");
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [toast, setToast] = useState<string | null>(null);
  const [actionsOpen, setActionsOpen] = useState(false);
  // Keyboard selection tracks the ITEM ID, not an index: a copy bumps the
  // card to the top and reorders the list, and an index would silently point
  // the detail pane at a different card. Missing ids fall back to the top row.
  const [selectedId, setSelectedId] = useState<number | null>(null);
  // Bumped on arrow-key moves so the list scrolls to follow the keyboard.
  const [scrollKey, setScrollKey] = useState(0);
  const selectedIdRef = useRef<number | null>(null);
  selectedIdRef.current = selectedId;
  const viewRef = useRef(view);
  viewRef.current = view;
  const hideOnBlurRef = useRef(true);
  // White-screen audit: incremented on every window focus transition. A blur
  // timer captures the epoch when armed; if it changed by fire time, a newer
  // focus/show superseded the timer and the hide must not run.
  const focusEpochRef = useRef(0);
  const debouncedRef = useRef("");
  debouncedRef.current = debounced;
  const categoryRef = useRef<Category | "all">("all");
  categoryRef.current = category;
  const settingsRef = useRef<AppSettings | null>(null);
  settingsRef.current = settings;
  const inputRef = useRef<HTMLInputElement>(null);
  const requestId = useRef(0);
  const themeWrapped = useRef(false);
  const actionsRef = useRef<HTMLDivElement>(null);

  const fetchFor = useCallback((query: string) => {
    const id = ++requestId.current;
    const p = query.trim()
      ? api.searchHistory(query.trim())
      : api.getHistory();
    p.then((rows) => {
      // Drop stale responses when typing fast (last-wins race).
      if (requestId.current === id) {
        setItems([...rows].sort(byPinnedThenRecent));
        setVisibleCount(50);
        setSelectedId(null); // falls back to the top row
      }
    }).catch(console.error);
  }, []);

  const refresh = useCallback(() => {
    fetchFor(debounced);
  }, [debounced, fetchFor]);

  useEffect(() => {
    const t = setTimeout(() => setDebounced(search), 200);
    return () => clearTimeout(t);
  }, [search]);

  useEffect(() => {
    fetchFor(debounced);
  }, [debounced, fetchFor]);

  useEffect(() => {
    api
      .getSettings()
      .then((s) => {
        setSettings(s);
        hideOnBlurRef.current = s.hide_on_blur;
      })
      .catch(console.error);
  }, []);

  useEffect(() => {
    const unlisten = listen<ClipboardItem>(EVENTS.newItem, (e) => {
      const item = e.payload;
      // A filtered view (search / type) must not have out-of-filter rows
      // pushed in — otherwise any capture made while a search is active
      // pops into the results and lingers there. Refetch and let the
      // backend decide what matches.
      if (debouncedRef.current.trim() || categoryRef.current !== "all") {
        fetchFor(debouncedRef.current);
        return;
      }
      setItems((prev) => placeItem(prev, item));
    });
    return () => {
      unlisten.then((fn) => fn());
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // popup behavior: Esc hides, losing focus hides, gaining focus refocuses search.
  useEffect(() => {
    if (!appWindow) return;
    const win = appWindow;
    let hideTimer: ReturnType<typeof setTimeout> | null = null;

    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        const target = e.target as HTMLElement | null;
        const inSearch = !!target && target.tagName === "INPUT" && target === inputRef.current;
        if (viewRef.current === "settings") {
          setView("list");
        } else if (inSearch && search) {
          setSearch("");
        } else {
          win.hide().catch(console.error);
        }
        return;
      }
      const target = e.target as HTMLElement | null;
      const typing =
        !!target && (target.tagName === "INPUT" || target.tagName === "TEXTAREA");
      if (
        matchesCombo(e, settingsRef.current?.actions_hotkey ?? "Ctrl+K") &&
        !typing
      ) {
        if (viewRef.current !== "list") return;
        e.preventDefault();
        setActionsOpen((o) => !o);
        return;
      }
      // Keyboard nav: list view only — settings has its own inputs.
      if (viewRef.current !== "list") return;
      const rows = visibleRef.current;
      if (rows.length === 0) return;
      if (e.key === "ArrowDown" || e.key === "ArrowUp") {
        // Always steer selection, even from the search box (launcher behavior).
        e.preventDefault();
        const delta = e.key === "ArrowDown" ? 1 : -1;
        const cur = rows.findIndex((r) => r.id === selectedIdRef.current);
        const next = moveSelection(cur < 0 ? 0 : cur, delta, rows.length);
        setSelectedId(rows[next].id);
        setScrollKey((k) => k + 1);
      } else if (e.key === "Enter") {
        // Enter pastes into the previous app (PR2); the toolbar button copies.
        const item = rows.find((r) => r.id === selectedIdRef.current) ?? rows[0];
        if (item) {
          e.preventDefault();
          pasteItemRef.current(item);
        }
      } else if (e.key === "Delete" && !typing) {
        const item = rows.find((r) => r.id === selectedIdRef.current);
        if (item) {
          e.preventDefault();
          removeItemRef.current(item);
        }
      }
    };
    window.addEventListener("keydown", onKey);
    const onOutsideClick = (e: MouseEvent) => {
      if (!actionsRef.current?.contains(e.target as Node)) setActionsOpen(false);
    };
    document.addEventListener("click", onOutsideClick);
    const focusUnlisten = win.onFocusChanged(({ payload: focused }) => {
      focusEpochRef.current += 1;
      if (hideTimer) {
        clearTimeout(hideTimer);
        hideTimer = null;
      }
      if (!focused) {
        // hide_on_blur off = popup stays until Esc or the hotkey.
        if (!hideOnBlurRef.current) return;
        const armedEpoch = focusEpochRef.current;
        hideTimer = setTimeout(() => {
          hideTimer = null;
          // Stale-hide protection (white-screen audit): a hide timer may only
          // fire into the exact state it was armed for. A newer focus/show
          // supersedes it; an already-hidden window is never hidden again; an
          // unverifiable state is skipped (never guess-hide — a hide landing
          // right after a show can leave the WebView2 unrendered).
          if (focusEpochRef.current !== armedEpoch) return;
          win.isFocused().then((focused) => {
            if (focused) return;
            return win.isVisible().then((visible) => {
              if (!visible) return;
              // Reset while hidden so the next summon paints the list on its
              // very first frame — never a flash of the settings page.
              setView("list");
              win.hide().catch(console.error);
            });
          }).catch((e) => console.error("blur-hide state check failed", e));
        }, 150);
      } else {
        setSearch("");
        // fetchFor("") refreshes + resets pagination
        fetchFor("");
        // Defer so the window finishes focusing before we steal it.
        requestAnimationFrame(() => inputRef.current?.focus());
      }
    });
    return () => {
      window.removeEventListener("keydown", onKey);
      document.removeEventListener("click", onOutsideClick);
      if (hideTimer) clearTimeout(hideTimer);
      focusUnlisten.then((fn) => fn());
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [fetchFor, search]);

  useEffect(() => {
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const apply = (mode: ThemeMode) => {
      const isDark = mode === "dark" || (mode === "system" && media.matches);
      document.documentElement.classList.toggle("dark", isDark);
      document.documentElement.style.colorScheme = isDark ? "dark" : "light";
    };

    localStorage.setItem(THEME_STORAGE_KEY, theme);

    const doc = document as Document & {
      startViewTransition?: (cb: () => void) => void;
    };
    // First mount: pre-paint script already set the class — just sync
    // colorScheme, no transition.
    if (!themeWrapped.current) {
      themeWrapped.current = true;
      const isDark = document.documentElement.classList.contains("dark");
      document.documentElement.style.colorScheme = isDark ? "dark" : "light";
    } else if (doc.startViewTransition) {
      doc.startViewTransition(() => apply(theme));
    } else {
      apply(theme);
    }

    // Only follow the OS while the user chose "system".
    if (theme !== "system") return;
    const onChange = () => apply("system");
    media.addEventListener("change", onChange);
    return () => media.removeEventListener("change", onChange);
  }, [theme]);

  const filtered = useMemo(
    () => (category === "all" ? items : items.filter((i) => i.category === category)),
    [items, category],
  );
  const visible = useMemo(() => filtered.slice(0, visibleCount), [filtered, visibleCount]);
  const visibleRef = useRef(visible);
  visibleRef.current = visible;
  const selectedItem =
    visible.find((i) => i.id === selectedId) ?? visible[0] ?? null;

  const showToast = useCallback((msg: string) => {
    setToast(msg);
    window.setTimeout(() => setToast((t) => (t === msg ? null : t)), 1300);
  }, []);

  // Card actions. Copy goes by id (backend loads the full row); images keep
  // the path-based command. The backend bump + live event move the card to
  // the top — the UI never waits for the round-trip.
  const copyItem = useCallback(
    (item: ClipboardItem) => {
      const p =
        item.kind === "image"
          ? api.copyImageToClipboard(item.content)
          : item.kind === "file"
            ? api.copyToClipboard(item.content) // path lists are never truncated
            : api.copyHistoryItem(item.id);
      p.then(() => showToast("Copied to clipboard")).catch(console.error);
    },
    [showToast],
  );

  const pasteItem = useCallback(
    (item: ClipboardItem) => {
      if (item.kind === "image") {
        copyItem(item);
        return;
      }
      if (item.kind === "file") {
        const paths = item.content.split("\n").filter(Boolean);
        if (paths.length === 0) return;
        const p =
          paths.length === 1 ? openPath(paths[0]) : revealItemInDir(paths[0]);
        p.catch(console.error);
        return;
      }
      api.pasteHistoryItem(item.id).catch(console.error);
    },
    [copyItem],
  );

  // Links open in the browser; Explorer file rows open/reveal in Explorer.
  const openUrlItem = useCallback(
    (url: string) => {
      openUrl(url)
        .then(() => showToast("Opening link…"))
        .catch(console.error);
    },
    [showToast],
  );
  const openPathItem = useCallback(
    (path: string) => {
      openPath(path)
        .then(() => showToast("Opening…"))
        .catch(console.error);
    },
    [showToast],
  );
  const openItem = useCallback(
    (item: ClipboardItem) => {
      if (item.category === "link") {
        openUrlItem(item.content);
      } else if (item.kind === "file") {
        const paths = item.content.split("\n").filter(Boolean);
        if (paths.length === 0) return;
        // Single entry opens with its app; several reveal the first in Explorer.
        const p =
          paths.length === 1 ? openPath(paths[0]) : revealItemInDir(paths[0]);
        p.then(() => showToast(paths.length === 1 ? "Opened" : "Revealed in Explorer")).catch(
          console.error,
        );
      }
    },
    [openPathItem, openUrlItem, showToast],
  );
  // Single click only selects. Actions are explicit: double-click copies
  // (paths as text for file rows), links/files open via the detail pane's
  // Open button — accidental clicks must never fire the browser/Explorer.
  const dblCopyItem = useCallback(
    (item: ClipboardItem) => {
      if (item.kind === "file") {
        api
          .copyToClipboard(item.content)
          .then(() => showToast("Path copied"))
          .catch(console.error);
      } else {
        copyItem(item);
      }
    },
    [copyItem, showToast],
  );

  // Pin and delete update local state FIRST — the DB round trip is sub-
  // millisecond but the follow-up list refetch re-reads the whole history;
  // waiting for it made delete/pin feel seconds-slow. The background refetch
  // reconciles; on error it also restores the truth from the backend.
  const pinItem = useCallback(
    (item: ClipboardItem) => {
      setItems((prev) =>
        [...prev.map((i) => (i.id === item.id ? { ...i, pinned: !i.pinned } : i))].sort(
          byPinnedThenRecent,
        ),
      );
      api.togglePin(item.id).then(refresh).catch((e) => {
        console.error(e);
        refresh();
      });
    },
    [refresh],
  );
  const removeItem = useCallback(
    (item: ClipboardItem) => {
      if (item.kind === "image") evictCachedImage(item.content);
      setItems((prev) => prev.filter((i) => i.id !== item.id));
      api
        .deleteItem(item.id)
        .then(refresh)
        .catch((e) => {
          console.error(e);
          refresh();
        });
    },
    [refresh],
  );
  const copyItemRef = useRef(copyItem);
  copyItemRef.current = copyItem;
  const removeItemRef = useRef(removeItem);
  removeItemRef.current = removeItem;
  const pasteItemRef = useRef(pasteItem);
  pasteItemRef.current = pasteItem;

  const handleCategory = useCallback((c: Category | "all") => {
    setCategory(c);
    setVisibleCount(50);
    setSelectedId(null);
  }, []);

  const appearance: { mode: ThemeMode; label: string; icon: typeof Sun }[] = [
    { mode: "light", label: "Light", icon: Sun },
    { mode: "system", label: "System", icon: Laptop },
    { mode: "dark", label: "Dark", icon: Moon },
  ];

  return (
    <main className="flex h-screen min-w-0 flex-col overflow-hidden bg-background text-foreground antialiased">
      {/* Dedicated grab strip: the top bar's interactive controls fill it,
          so this margin is what makes the frameless window easy to drag. */}
      <div data-tauri-drag-region aria-hidden="true" className="h-3 shrink-0" />
      {view === "settings" ? (
        <div
          data-tauri-drag-region
          className="flex h-11 shrink-0 items-center gap-2 border-b border-border/60 px-3"
        >
          <button
            type="button"
            aria-label="Back to history"
            onClick={() => setView("list")}
            className="grid size-7 place-items-center rounded-lg text-muted-foreground transition-colors hover:bg-foreground/5 hover:text-foreground"
          >
            <ArrowLeft className="size-4" />
          </button>
          <span data-tauri-drag-region className="text-[13px] font-semibold">
            Settings
          </span>
        </div>
      ) : (
        <div
          data-tauri-drag-region
          className="flex h-11 shrink-0 items-center gap-2.5 border-b border-border/60 px-3"
        >
          <div className="flex min-w-0 flex-1 items-center gap-2 rounded-lg bg-muted/70 px-2.5 py-1.5 transition-shadow focus-within:bg-muted focus-within:shadow-[0_0_0_3px_rgba(10,132,255,0.15)]">
            <SearchIcon className="size-3.5 shrink-0 text-muted-foreground" />
            <input
              ref={inputRef}
              type="text"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              placeholder="Type to filter entries…"
              autoComplete="off"
              className="min-w-0 flex-1 border-0 bg-transparent text-[13px] text-foreground outline-none placeholder:text-muted-foreground"
            />
            {search && (
              <button
                type="button"
                aria-label="Clear search"
                title="Clear"
                onClick={() => {
                  setSearch("");
                  inputRef.current?.focus();
                }}
                className="grid size-4 shrink-0 place-items-center rounded-full text-muted-foreground transition-colors hover:bg-foreground/10 hover:text-foreground"
              >
                <X className="size-3" />
              </button>
            )}
          </div>
          <TypeDropdown value={category} onChange={handleCategory} />
        </div>
      )}

      {view === "settings" ? (
        <div className="min-h-0 flex-1 overflow-y-auto p-3">
          {settings ? (
            <SettingsPanel
              initial={settings}
              onSaved={(s) => {
                setSettings(s);
                hideOnBlurRef.current = s.hide_on_blur;
                refresh();
              }}
              onCleared={refresh}
            />
          ) : (
            <div className="grid h-full place-items-center text-sm text-muted-foreground">
              Loading settings…
            </div>
          )}
        </div>
      ) : (
        <div className="flex min-h-0 flex-1">
          <EntryList
            items={visible}
            selectedId={selectedItem?.id ?? null}
            scrollKey={scrollKey}
            totalCount={filtered.length}
            visibleCount={visibleCount}
            onShowMore={() => setVisibleCount((n) => n + 50)}
            onSelect={(id) => setSelectedId(id)}
            onDblCopy={dblCopyItem}
          />
          <DetailPane
            item={selectedItem}
            onPin={pinItem}
            onDelete={removeItem}
            onOpen={openItem}
            onOpenUrl={openUrlItem}
            onOpenPath={openPathItem}
          />
        </div>
      )}

      {view === "list" && (
        <div
          data-tauri-drag-region
          className="flex h-11 shrink-0 items-center gap-2.5 border-t border-border/60 px-3"
        >
          <div
            data-tauri-drag-region
            className="flex shrink-0 items-center gap-2 text-[12px] font-semibold"
          >
            <span
              aria-hidden="true"
              className="grid size-[22px] place-items-center rounded-md bg-gradient-to-br from-rose-400 to-rose-600 text-white shadow-[0_2px_6px_rgba(255,55,95,0.4)]"
            >
              <ClipboardIcon className="size-3" />
            </span>
            <span data-tauri-drag-region>Clipboard History</span>
          </div>
          <button
            type="button"
            onClick={() => selectedItem && copyItem(selectedItem)}
            disabled={!selectedItem}
            className="mx-auto flex items-center gap-2 rounded-lg bg-foreground px-4 py-[7px] text-[12.5px] font-semibold text-background transition-all duration-150 hover:opacity-90 active:scale-[0.97] disabled:opacity-40"
          >
            <Copy className="size-3.5" />
            Copy to Clipboard
          </button>
          <div ref={actionsRef} className="relative shrink-0">
            <button
              type="button"
              onClick={() => setActionsOpen((o) => !o)}
              className="flex items-center gap-1.5 rounded-lg px-2 py-1 text-[12px] text-muted-foreground transition-colors hover:text-foreground"
            >
              <Keyboard className="size-3.5" />
              Actions
              {(settings?.actions_hotkey ?? "Ctrl+K")
                .split("+")
                .map((part) => (
                  <kbd
                    key={part}
                    className="rounded border border-border/80 px-1 py-px font-sans text-[9.5px]"
                  >
                    {part.trim()}
                  </kbd>
                ))}
            </button>
            {actionsOpen && (
              <div className="absolute bottom-[calc(100%+8px)] right-0 z-30 w-[170px] rounded-xl border border-border/80 bg-popover p-1 shadow-xl shadow-black/25">
                <button
                  type="button"
                  onClick={() => {
                    setActionsOpen(false);
                    setView("settings");
                  }}
                  className="flex w-full items-center gap-2 rounded-lg px-2 py-1.5 text-left text-[12.5px] transition-colors hover:bg-foreground/5"
                >
                  <SettingsIcon className="size-3.5 text-muted-foreground" />
                  Open Settings
                </button>
                <div className="mx-2 my-1 border-t border-border/50" />
                <div className="flex items-center justify-between px-2 py-1">
                  <span className="text-[11px] text-muted-foreground">Appearance</span>
                  <div className="flex gap-0.5">
                    {appearance.map(({ mode, label, icon: Icon }) => (
                      <button
                        key={mode}
                        type="button"
                        aria-label={label}
                        aria-pressed={theme === mode}
                        onClick={() => setTheme(mode)}
                        className={cn(
                          "grid size-6 place-items-center rounded-md text-muted-foreground transition-colors hover:bg-foreground/5 hover:text-foreground",
                          theme === mode && "bg-foreground/10 text-foreground",
                        )}
                      >
                        <Icon className="size-3.5" />
                      </button>
                    ))}
                  </div>
                </div>
              </div>
            )}
          </div>
        </div>
      )}

      {toast && (
        <div className="pointer-events-none fixed bottom-14 left-1/2 -translate-x-1/2 rounded-full bg-foreground px-3.5 py-[7px] text-[12px] font-medium text-background shadow-[0_8px_24px_rgba(0,0,0,0.25)]">
          {toast}
        </div>
      )}
    </main>
  );
}

export default App;
