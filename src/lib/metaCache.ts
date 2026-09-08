import { api } from "./api";
import type { FileMeta } from "./types";

// In-memory stat cache mirroring imageCache: one IPC stat per unique path
// list, shared across re-renders and filter changes. Entries are tiny
// (counts + missing names), capped like the thumbnail cache.
const cache = new Map<string, FileMeta>();
const inflight = new Map<string, Promise<FileMeta>>();

export function getCachedFileMeta(content: string): Promise<FileMeta> {
  const hit = cache.get(content);
  if (hit) return Promise.resolve(hit);
  const pending = inflight.get(content);
  if (pending) return pending;
  const p = api
    .fileMeta(content.split("\n").filter(Boolean))
    .then((meta) => {
      if (cache.size >= 200) {
        const oldest = cache.keys().next().value;
        if (oldest) cache.delete(oldest);
      }
      cache.set(content, meta);
      inflight.delete(content);
      return meta;
    })
    .catch((e) => {
      inflight.delete(content);
      throw e;
    });
  inflight.set(content, p);
  return p;
}
