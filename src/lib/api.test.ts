import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => []) }));

import { api } from "./api";
import { invoke } from "@tauri-apps/api/core";

const mockedInvoke = vi.mocked(invoke);

describe("api", () => {
  beforeEach(() => {
    mockedInvoke.mockClear();
  });

  it("calls get_history", async () => {
    await api.getHistory();
    expect(mockedInvoke).toHaveBeenCalledWith("get_history");
  });

  it("passes query to search_history", async () => {
    await api.searchHistory("foo");
    expect(mockedInvoke).toHaveBeenCalledWith("search_history", { query: "foo" });
  });

  it("passes id to delete_item", async () => {
    await api.deleteItem(7);
    expect(mockedInvoke).toHaveBeenCalledWith("delete_item", { id: 7 });
  });

  it("passes id to toggle_pin", async () => {
    await api.togglePin(3);
    expect(mockedInvoke).toHaveBeenCalledWith("toggle_pin", { id: 3 });
  });

  it("passes text to copy_to_clipboard", async () => {
    await api.copyToClipboard("hello");
    expect(mockedInvoke).toHaveBeenCalledWith("copy_to_clipboard", { text: "hello" });
  });

  it("passes path to read_image_base64", async () => {
    await api.readImageBase64("C:\\img\\a.png");
    expect(mockedInvoke).toHaveBeenCalledWith("read_image_base64", {
      path: "C:\\img\\a.png",
    });
  });
});
