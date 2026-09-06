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
