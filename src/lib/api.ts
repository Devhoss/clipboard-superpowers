import { invoke } from "@tauri-apps/api/core";
import type { ClipboardItem, AppSettings, HistoryStats, FileMeta } from "./types";

export const api = {
  getHistory: (): Promise<ClipboardItem[]> => invoke("get_history"),
  searchHistory: (query: string): Promise<ClipboardItem[]> =>
    invoke("search_history", { query }),
  deleteItem: (id: number): Promise<void> => invoke("delete_item", { id }),
  togglePin: (id: number): Promise<void> => invoke("toggle_pin", { id }),
  // Full row (list payloads carry 300-char content previews) — the detail
  // pane loads this when the selection changes.
  getHistoryItem: (id: number): Promise<ClipboardItem | null> =>
    invoke("get_history_item", { id }),
  copyToClipboard: (text: string): Promise<void> =>
    invoke("copy_to_clipboard", { text }),
  // Copy a history row by id: the backend loads the full text/html itself —
  // list payloads carry only 300-char previews (multi-MB savings).
  copyHistoryItem: (id: number): Promise<void> =>
    invoke("copy_history_item", { id }),
  copyImageToClipboard: (path: string): Promise<void> =>
    invoke("copy_image_to_clipboard", { path }),
  // PR2: copy-then-paste into the previous app, by history id.
  pasteHistoryItem: (id: number): Promise<void> =>
    invoke("paste_history_item", { id }),
  readImageBase64: (path: string): Promise<string> =>
    invoke("read_image_base64", { path }),
  // PR6: local OCR on one of our image cards. Returns the extracted text;
  // the new text item arrives separately through the live event.
  ocrImage: (path: string): Promise<string> => invoke("ocr_image", { path }),
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
