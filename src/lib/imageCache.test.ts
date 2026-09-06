import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("./api", () => ({ api: { readImageBase64: vi.fn() } }));

import { api } from "./api";
import { getCachedImage, evictCachedImage } from "./imageCache";

const mocked = vi.mocked(api.readImageBase64);

describe("imageCache", () => {
  beforeEach(() => {
    mocked.mockReset();
    evictCachedImage("a.png");
    evictCachedImage("b.png");
  });

  it("caches base64 so second call does not re-read", async () => {
    mocked.mockResolvedValue("AAA");
    const first = await getCachedImage("a.png");
    const second = await getCachedImage("a.png");
    expect(first).toBe("data:image/png;base64,AAA");
    expect(second).toBe(first);
    expect(mocked).toHaveBeenCalledTimes(1);
  });

  it("evict forces a re-read", async () => {
    mocked.mockResolvedValueOnce("AAA").mockResolvedValueOnce("BBB");
    await getCachedImage("b.png");
    evictCachedImage("b.png");
    const again = await getCachedImage("b.png");
    expect(again).toBe("data:image/png;base64,BBB");
    expect(mocked).toHaveBeenCalledTimes(2);
  });
});
