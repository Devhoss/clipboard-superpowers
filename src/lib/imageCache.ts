import { api } from "./api";

// In-memory base64 cache so scrolling a long image list doesn't re-read
// every PNG from disk on each mount. Cleared when an image is deleted.
const cache = new Map<string, string>();
const inflight = new Map<string, Promise<string>>();

export function getCachedImage(path: string): Promise<string> {
  const hit = cache.get(path);
  if (hit) return Promise.resolve(hit);
  const pending = inflight.get(path);
  if (pending) return pending;
  const p = api
    .readImageBase64(path)
    .then((b64) => {
      const url = `data:image/png;base64,${b64}`;
      // Cap cache to ~100 thumbnails to bound memory.
      if (cache.size >= 100) {
        const oldest = cache.keys().next().value;
        if (oldest) cache.delete(oldest);
      }
      cache.set(path, url);
      inflight.delete(path);
      return url;
    })
    .catch((e) => {
      inflight.delete(path);
      throw e;
    });
  inflight.set(path, p);
  return p;
}

export function evictCachedImage(path: string) {
  cache.delete(path);
}
