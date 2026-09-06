export type Category = "plain" | "link" | "code" | "color" | "email" | "image";
export type Kind = "text" | "image";

export interface ClipboardItem {
  id: number;
  content: string;
  content_hash: string;
  category: Category;
  kind: Kind;
  pinned: boolean;
  created_at: string;
}

export const CATEGORIES: Category[] = ["plain", "link", "code", "color", "email", "image"];

export interface AppSettings {
  hotkey: string;
  max_items: number;
  launch_on_login: boolean;
  capture_text: boolean;
  capture_images: boolean;
  hide_on_blur: boolean;
}

export interface HistoryStats {
  total: number;
  pinned: number;
  db_bytes: number;
}
