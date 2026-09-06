import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { SearchBar } from "./components/SearchBar";
import { CategoryFilter } from "./components/CategoryFilter";
import { HistoryList } from "./components/HistoryList";
import { ThemeSwitcher, type ThemeMode } from "./components/ThemeSwitcher";
import { api, EVENTS } from "./lib/api";
import type { ClipboardItem } from "./lib/types";

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
  const inputRef = useRef<HTMLInputElement>(null);
  const requestId = useRef(0);

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
  // Single onFocusChanged subscription handles both directions — the old
  // separate `tauri://focus` listener never fired reliably.
  useEffect(() => {
    if (!appWindow) return;

    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") appWindow.hide().catch(console.error);
    };
    window.addEventListener("keydown", onKey);
    const focusUnlisten = appWindow.onFocusChanged(({ payload: focused }) => {
      if (!focused) {
        appWindow.hide().catch(console.error);
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
      focusUnlisten.then((fn) => fn());
    };
  }, [fetchFor]);

  useEffect(() => {
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const updateAppearance = () => {
      const isDark = theme === "dark" || (theme === "system" && media.matches);
      document.documentElement.classList.toggle("dark", isDark);
    };

    updateAppearance();
    localStorage.setItem(THEME_STORAGE_KEY, theme);
    media.addEventListener("change", updateAppearance);
    return () => media.removeEventListener("change", updateAppearance);
  }, [theme]);

  const filtered = useMemo(
    () => (category === "all" ? items : items.filter((i) => i.category === category)),
    [items, category],
  );
  const visible = useMemo(() => filtered.slice(0, visibleCount), [filtered, visibleCount]);

  const handleCategory = useCallback((c: string) => {
    setCategory(c);
    setVisibleCount(50);
  }, []);

  return (
    <main className="flex h-screen min-w-0 flex-col gap-2 overflow-hidden bg-background p-2 text-foreground antialiased">
      <div
        className="flex h-6 shrink-0 items-center justify-between select-none"
      >
        <div
          data-tauri-drag-region
          className="flex min-w-0 cursor-grab items-center gap-2"
        >
          <span data-tauri-drag-region className="size-1.5 rounded-full bg-primary/80 shadow-[0_0_8px_color-mix(in_oklch,var(--primary),transparent_35%)]" />
          <span data-tauri-drag-region className="text-xs font-semibold tracking-[-0.01em]">Clipboard</span>
        </div>
        <div className="flex shrink-0 items-center gap-2">
          <span className="text-[10px] text-muted-foreground">
            Ctrl+Alt+V
          </span>
          <ThemeSwitcher value={theme} onChange={setTheme} />
        </div>
      </div>
      <div className="shrink-0 space-y-2">
        <SearchBar value={search} onChange={setSearch} inputRef={inputRef} />
        <CategoryFilter value={category} onChange={handleCategory} />
      </div>
      {filtered.length > 0 ? (
        <>
          <HistoryList items={visible} onMutate={refresh} />
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
    </main>
  );
}

export default App;
