import type { Category } from "./types";

/** Category identity: dot color + singular/plural labels (v2 design). */
export const CATEGORY_META: Record<
  Category,
  { single: string; plural: string; dot: string; text: string }
> = {
  plain: { single: "Text", plural: "Text", dot: "#98989d", text: "text-zinc-400" },
  link: { single: "Link", plural: "Links", dot: "#0a84ff", text: "text-sky-300" },
  code: { single: "Code", plural: "Code", dot: "#bf5af2", text: "text-purple-300" },
  color: { single: "Color", plural: "Colors", dot: "#ff375f", text: "text-rose-300" },
  email: { single: "Email", plural: "Emails", dot: "#ff9f0a", text: "text-amber-300" },
  image: { single: "Image", plural: "Images", dot: "#30d158", text: "text-green-300" },
  secret: { single: "Secret", plural: "Secrets", dot: "#ff453a", text: "text-red-300" },
  file: { single: "File", plural: "Files", dot: "#64d2ff", text: "text-sky-200" },
};
