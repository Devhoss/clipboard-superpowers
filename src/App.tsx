import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { SearchBar } from "./components/SearchBar";
import { CategoryFilter } from "./components/CategoryFilter";
import { HistoryList } from "./components/HistoryList";
import { api, EVENTS } from "./lib/api";
import type { ClipboardItem } from "./lib/types";

const appWindow = getCurrentWebviewWindow();

function App() {
  const [items, setItems] = useState<ClipboardItem[]>([]);
  const [search, setSearch] = useState("");
  const [debounced, setDebounced] = useState("");
  const [category, setCategory] = useState("all");
  const inputRef = useRef<HTMLInputElement>(null);

  const refresh = useCallback(() => {
    (search ? api.searchHistory(search) : api.getHistory())
      .then(setItems)
      .catch(console.error);
  }, [search]);

  useEffect(() => {
    api.getHistory().then(setItems).catch(console.error);
  }, []);

  useEffect(() => {
    const t = setTimeout(() => setDebounced(search), 200);
    return () => clearTimeout(t);
  }, [search]);

  useEffect(() => {
    if (debounced.trim()) {
      api.searchHistory(debounced).then(setItems).catch(console.error);
    } else {
      api.getHistory().then(setItems).catch(console.error);
    }
  }, [debounced]);

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

  // popup behavior: Esc hides, losing focus hides, showing refocuses the search box
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") appWindow.hide().catch(console.error);
    };
    window.addEventListener("keydown", onKey);
    const blurUnlisten = appWindow.onFocusChanged(({ payload: focused }) => {
      if (!focused) appWindow.hide().catch(console.error);
    });
    const focusUnlisten = appWindow.window
      .listen("tauri://focus", () => {
        setSearch("");
        inputRef.current?.focus();
        refresh();
      })
    return () => {
      window.removeEventListener("keydown", onKey);
      blurUnlisten.then((fn) => fn());
      focusUnlisten.then((fn) => fn());
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const filtered = useMemo(
    () => (category === "all" ? items : items.filter((i) => i.category === category)),
    [items, category],
  );

  return (
    <main className="flex h-screen flex-col gap-2 bg-background p-2 text-foreground">
      <div
        data-tauri-drag-region
        className="flex h-5 shrink-0 items-center justify-between select-none"
      >
        <span data-tauri-drag-region className="text-xs font-medium text-muted-foreground">
          Clipboard Superpowers
        </span>
        <span data-tauri-drag-region className="text-[10px] text-muted-foreground">
          Ctrl+Alt+V to toggle
        </span>
      </div>
      <div className="shrink-0 space-y-2">
        <SearchBar value={search} onChange={setSearch} inputRef={inputRef} />
        <CategoryFilter value={category} onChange={setCategory} />
      </div>
      {filtered.length > 0 ? (
        <HistoryList items={filtered} onMutate={refresh} />
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
