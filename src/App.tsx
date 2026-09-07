import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { SearchBar } from "./components/SearchBar";
import { CategoryFilter } from "./components/CategoryFilter";
import { HistoryList } from "./components/HistoryList";
import { SettingsPanel } from "./components/SettingsPanel";
import { ThemeSwitcher, type ThemeMode } from "./components/ThemeSwitcher";
import { api, EVENTS } from "./lib/api";
import { clampSelection, moveSelection } from "./lib/keyboardNav";
import { evictCachedImage } from "@/lib/imageCache";
import type { AppSettings, ClipboardItem } from "./lib/types";
import { ArrowLeft, Settings as SettingsIcon } from "lucide-react";

// Keep the renderer usable while it is being previewed in a regular browser.
// Tauri injects this bridge before the app loads, but it is intentionally absent
// from Vite's browser preview. Creating a window handle without it throws before
// React gets a chance to render anything.
const appWindow =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window
    ? getCurrentWebviewWindow()
    : null;

const THEME_STORAGE_KEY = "clipboard-superpowers-theme";

function getInitialTheme(): ThemeMode {
  const stored = localStorage.getItem(THEME_STORAGE_KEY);
  return stored === "light" || stored === "dark" || stored === "system" ? stored : "system";
}

function App() {
  const [items, setItems] = useState<ClipboardItem[]>([]);
  const [search, setSearch] = useState("");
  const [debounced, setDebounced] = useState("");
  const [category, setCategory] = useState("all");
  const [theme, setTheme] = useState<ThemeMode>(getInitialTheme);
  const [visibleCount, setVisibleCount] = useState(50);
  const [view, setView] = useState<"list" | "settings">("list");
  const [settings, setSettings] = useState<AppSettings | null>(null);
  // Keyboard selection (PR1). Index into `visible` below, not the full list.
  const [selectedIndex, setSelectedIndex] = useState(0);
  const selectedRef = useRef(0);
  selectedRef.current = selectedIndex;
  const viewRef = useRef(view);
  viewRef.current = view;
  const hideOnBlurRef = useRef(true);
  const inputRef = useRef<HTMLInputElement>(null);
  const requestId = useRef(0);
  const themeWrapped = useRef(false);

  const fetchFor = useCallback((query: string) => {
    const id = ++requestId.current;
    const p = query.trim()
      ? api.searchHistory(query.trim())
      : api.getHistory();
    p.then((rows) => {
      // Drop stale responses when typing fast (last-wins race).
      if (requestId.current === id) {
        setItems(rows);
        setVisibleCount(50);
        setSelectedIndex(0);
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
      setItems((prev) => {
        const rest = prev.filter((i) => i.id !== item.id);
        return [item, ...rest];
      });
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  // popup behavior: Esc hides, losing focus hides, gaining focus refocuses search.
  // Hide is debounced + re-verified: the synthetic title-bar mousedown Tauri
  // sends for drag-region dragging can fire a transient blur — hiding
  // instantly would kill the window on the click that should start a drag.
  useEffect(() => {
    if (!appWindow) return;
    const win = appWindow;
    let hideTimer: ReturnType<typeof setTimeout> | null = null;

    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        // In settings, Esc goes back first — only hides from the list.
        if (viewRef.current === "settings") {
          setView("list");
        } else {
          win.hide().catch(console.error);
        }
        return;
      }
      // Keyboard nav (PR1): list view only — settings has its own inputs.
      if (viewRef.current !== "list") return;
      const rows = visibleRef.current;
      if (rows.length === 0) return;
      const target = e.target as HTMLElement | null;
      const typing =
        !!target && (target.tagName === "INPUT" || target.tagName === "TEXTAREA");
      if (e.key === "ArrowDown" || e.key === "ArrowUp") {
        // Always steer selection, even from the search box (launcher
        // behavior). Left/Right stay native for caret movement.
        e.preventDefault();
        const delta = e.key === "ArrowDown" ? 1 : -1;
        setSelectedIndex((i) => moveSelection(i, delta, rows.length));
      } else if (e.key === "Enter") {
        // Enter is inert in a single-line search box, so copying the
        // selected card from there is safe and useful.
        const item = rows[selectedRef.current] ?? rows[0];
        if (item) {
          e.preventDefault();
          copyItemRef.current(item);
        }
      } else if (e.key === "Delete" && !typing) {
        // Delete key only, never Backspace — hijacking Backspace while
        // typing would eat search text instead of deleting cards.
        const item = rows[selectedRef.current];
        if (item) {
          e.preventDefault();
          removeItemRef.current(item);
        }
      }
    };
    window.addEventListener("keydown", onKey);
    const focusUnlisten = win.onFocusChanged(({ payload: focused }) => {
      if (hideTimer) {
        clearTimeout(hideTimer);
        hideTimer = null;
      }
      if (!focused) {
        // hide_on_blur off = popup stays until Esc or the hotkey.
        if (!hideOnBlurRef.current) return;
        hideTimer = setTimeout(() => {
          // Still unfocused after the grace window? Then it's a real
          // click-away — hide. Transient drag blips re-focus first.
          const doHide = () => {
            // Reset while hidden so the next summon paints the list on its
            // very first frame — never a flash of the settings page.
            setView("list");
            win.hide().catch(console.error);
          };
          win.isFocused().then((focused) => {
            if (!focused) doHide();
          }).catch(doHide);
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
      if (hideTimer) clearTimeout(hideTimer);
      focusUnlisten.then((fn) => fn());
    };
  }, [fetchFor]);

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
      // Smooth cross-fade via View Transitions (Chromium/WebView2). Falls
      // back to an instant swap where unsupported — no staggered shimmer.
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

  // Clamp selection when the list shrinks (filter narrows, item deleted).
  useEffect(() => {
    setSelectedIndex((i) => clampSelection(i, visibleRef.current.length));
  }, [filtered.length, visibleCount]);

  const handleCategory = useCallback((c: string) => {
    setCategory(c);
    setVisibleCount(50);
    setSelectedIndex(0);
  }, []);

  // Instant move-to-top on card click. The backend bump + live event follow
  // within a tick and dedupe by id — the UI never waits for the round-trip.
  const moveToTop = useCallback((item: ClipboardItem) => {
    setItems((prev) => [item, ...prev.filter((i) => i.id !== item.id)]);
  }, []);

  // Card actions, lifted here so mouse AND keyboard share one path (PR1).
  // Same behavior as before the lift: optimistic updates, errors to console.
  const copyItem = useCallback(
    (item: ClipboardItem) => {
      moveToTop(item);
      const p =
        item.kind === "image"
          ? api.copyImageToClipboard(item.content)
          : api.copyToClipboard(item.content);
      p.catch(console.error);
    },
    [moveToTop],
  );
  const pinItem = useCallback(
    (item: ClipboardItem) => {
      api.togglePin(item.id).then(refresh).catch(console.error);
    },
    [refresh],
  );
  const removeItem = useCallback(
    (item: ClipboardItem) => {
      if (item.kind === "image") evictCachedImage(item.content);
      api
        .deleteItem(item.id)
        .then(refresh)
        .catch(console.error);
    },
    [refresh],
  );
  const copyItemRef = useRef(copyItem);
  copyItemRef.current = copyItem;
  const removeItemRef = useRef(removeItem);
  removeItemRef.current = removeItem;

  return (
    <main className="flex h-screen min-w-0 flex-col gap-2 overflow-hidden bg-background p-2 text-foreground antialiased">
      <div
        data-tauri-drag-region
        className="flex h-6 shrink-0 cursor-grab items-center justify-between active:cursor-grabbing select-none"
      >
        {view === "settings" ? (
          <button
            type="button"
            onClick={() => setView("list")}
            aria-label="Back to history"
            className="flex min-w-0 items-center gap-1.5 text-xs font-semibold text-foreground"
          >
            <ArrowLeft className="size-3.5" />
            <span>Settings</span>
          </button>
        ) : (
          <div className="flex min-w-0 items-center gap-2">
            <span data-tauri-drag-region className="size-1.5 rounded-full bg-primary/80 shadow-[0_0_8px_color-mix(in_oklch,var(--primary),transparent_35%)]" />
            <span data-tauri-drag-region className="text-xs font-semibold tracking-[-0.01em]">Clipboard</span>
          </div>
        )}
        <div className="flex shrink-0 items-center gap-2">
          <span className="text-[10px] text-muted-foreground">
            {settings?.hotkey ?? "Ctrl+Alt+V"}
          </span>
          {view === "list" && (
            <button
              type="button"
              onClick={() => setView("settings")}
              aria-label="Open settings"
              className="grid size-5 place-items-center rounded-md text-muted-foreground transition hover:bg-muted hover:text-foreground"
            >
              <SettingsIcon className="size-3" strokeWidth={1.8} />
            </button>
          )}
          <ThemeSwitcher value={theme} onChange={setTheme} />
        </div>
      </div>
      {view === "settings" ? (
        settings ? (
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
          <div className="flex flex-1 items-center justify-center text-sm text-muted-foreground">
            Loading settings…
          </div>
        )
      ) : (
        <>
          <div className="shrink-0 space-y-2">
            <SearchBar value={search} onChange={setSearch} inputRef={inputRef} />
            <CategoryFilter value={category} onChange={handleCategory} />
          </div>
          {filtered.length > 0 ? (
            <>
              <HistoryList
                items={visible}
                selectedIndex={selectedIndex}
                onCopy={copyItem}
                onPin={pinItem}
                onDelete={removeItem}
                onHoverIndex={setSelectedIndex}
              />
              {visibleCount < filtered.length && (
                <button
                  type="button"
                  onClick={() => setVisibleCount((n) => n + 50)}
                  className="shrink-0 rounded-lg border border-border/70 bg-muted/60 px-3 py-1.5 text-xs text-muted-foreground transition hover:bg-muted hover:text-foreground"
                >
                  Show more ({filtered.length - visibleCount} remaining)
                </button>
              )}
            </>
          ) : (
            <div className="flex flex-1 items-center justify-center text-sm text-muted-foreground">
              {search || category !== "all"
                ? "No matches."
                : "No clips yet — copy something!"}
            </div>
          )}
        </>
      )}
    </main>
  );
}

export default App;
