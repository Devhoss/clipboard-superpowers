import { invoke } from "@tauri-apps/api/core";
import type { ClipboardItem } from "./types";

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
  readImageBase64: (path: string): Promise<string> =>
    invoke("read_image_base64", { path }),
};

export const EVENTS = {
  newItem: "clipboard:new-item",
} as const;
