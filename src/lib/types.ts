export type Category = "plain" | "link" | "code" | "color" | "email" | "image" | "secret" | "file";
export type Kind = "text" | "image" | "file";
export type ThemeMode = "light" | "dark" | "system";

export interface ClipboardItem {
  id: number;
  content: string;
  content_hash: string;
  category: Category;
  kind: Kind;
  pinned: boolean;
  created_at: string;
  /** Sanitized HTML flavor (PR4). Null = plain text. */
  html: string | null;
  /** Friendly name of the app the clip was copied from. Null = unknown
   * (rows from before this existed, or the owner couldn't be read). */
  source_app: string | null;
}

export const CATEGORIES: Category[] = ["plain", "link", "code", "color", "email", "image", "secret", "file"];

export interface AppSettings {
  hotkey: string;
  /** In-app shortcut for the Actions menu (matched by the webview). */
  actions_hotkey: string;
  max_items: number;
  launch_on_login: boolean;
  capture_text: boolean;
  capture_images: boolean;
  hide_on_blur: boolean;
  skip_secrets: boolean;
  capture_files: boolean;
}

export interface HistoryStats {
  total: number;
  pinned: number;
  db_bytes: number;
}

export interface FileMeta {
  total_bytes: number;
  existing: number;
  /** Folders among existing (their stub sizes are excluded from total). */
  dirs: number;
  missing: string[];
}
