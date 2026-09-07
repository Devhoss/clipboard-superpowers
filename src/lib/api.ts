import { invoke } from "@tauri-apps/api/core";
import type { ClipboardItem, AppSettings, HistoryStats, FileMeta } from "./types";

export const api = {
  getHistory: (): Promise<ClipboardItem[]> => invoke("get_history"),
  searchHistory: (query: string): Promise<ClipboardItem[]> =>
    invoke("search_history", { query }),
  deleteItem: (id: number): Promise<void> => invoke("delete_item", { id }),
  togglePin: (id: number): Promise<void> => invoke("toggle_pin", { id }),
  copyToClipboard: (text: string): Promise<void> =>
    invoke("copy_to_clipboard", { text }),
  copyImageToClipboard: (path: string): Promise<void> =>
    invoke("copy_image_to_clipboard", { path }),
  // PR4: text + sanitized HTML flavor. Falls back to plain copy when html is null.
  copyRichToClipboard: (text: string, html: string): Promise<void> =>
    invoke("copy_rich_to_clipboard", { text, html }),
  // PR2: copy-then-paste into the previous app.
  pasteTextToPreviousApp: (text: string): Promise<void> =>
    invoke("paste_text_to_previous_app", { text }),
  readImageBase64: (path: string): Promise<string> =>
    invoke("read_image_base64", { path }),
  getSettings: (): Promise<AppSettings> => invoke("get_settings"),
  updateSettings: (settings: AppSettings): Promise<AppSettings> =>
    invoke("update_settings", { settings }),
  // Tauri camelCases multi-word arg keys (Rust `delete_pinned` <-> JS
  // `deletePinned`) — this is currently the only command with one.
  clearHistory: (delete_pinned: boolean): Promise<number> =>
    invoke("clear_history", { deletePinned: delete_pinned }),
  getStats: (): Promise<HistoryStats> => invoke("get_stats"),
  // PR5: stat-only metadata for a newline-joined path list. Never reads contents.
  fileMeta: (paths: string[]): Promise<FileMeta> => invoke("file_meta", { paths }),
};

export const EVENTS = {
  newItem: "clipboard:new-item",
} as const;
