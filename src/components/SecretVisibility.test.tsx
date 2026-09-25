import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { EntryList } from "./EntryList";
import { DetailPane } from "./DetailPane";
import type { ClipboardItem } from "@/lib/types";

const revealSecret = vi.fn(async (_id: number) => "password=fixture-value");

vi.mock("@/lib/api", () => ({
  api: {
    getHistoryItem: vi.fn(async () => null),
    revealSecret: (id: number) => revealSecret(id),
  },
}));

// What the backend actually sends for a secret row: the real content is NOT
// here — the preview carries a fixed placeholder and a blank hash.
const secretRow: ClipboardItem = {
  id: 1,
  content: "••••••••",
  content_hash: "",
  category: "secret",
  kind: "text",
  pinned: false,
  created_at: "2026-09-24T10:00:00Z",
  html: null,
  source_app: "Notepad",
};

const noop = () => {};

afterEach(() => {
  cleanup();
  revealSecret.mockClear();
});

describe("secret rows are listed but redacted", () => {
  it("shows the redacted placeholder, never secret bytes", () => {
    render(
      <EntryList
        items={[secretRow]}
        selectedId={null}
        scrollKey={0}
        totalCount={1}
        visibleCount={50}
        onShowMore={noop}
        onSelect={noop}
        onDblCopy={noop}
      />,
    );

    expect(screen.getByText("••••••••")).toBeTruthy();
    expect(screen.getByRole("option")).toBeTruthy();
    // The placeholder must not carry the real value in any form.
    expect(screen.queryByText(/fixture-value/)).toBeNull();
  });
});

describe("secret reveal", () => {
  const pane = (secretEpoch?: number) => (
    <DetailPane
      item={secretRow}
      onPin={noop}
      onDelete={noop}
      onOpen={noop}
      onOpenUrl={noop}
      onOpenPath={noop}
      secretEpoch={secretEpoch}
    />
  );

  it("hides the content until the reveal button is pressed", async () => {
    render(pane());
    expect(screen.getByText("Secret hidden")).toBeTruthy();
    expect(screen.queryByText(/fixture-value/)).toBeNull();
    expect(revealSecret).not.toHaveBeenCalled();

    await userEvent.click(screen.getByRole("button", { name: "Reveal secret" }));
    await waitFor(() => expect(screen.getByText(/fixture-value/)).toBeTruthy());
    expect(revealSecret).toHaveBeenCalledWith(1);
  });

  it("hides again when the eye button is pressed a second time", async () => {
    render(pane());
    await userEvent.click(screen.getByRole("button", { name: "Reveal secret" }));
    await waitFor(() => expect(screen.getByText(/fixture-value/)).toBeTruthy());

    await userEvent.click(screen.getByRole("button", { name: "Hide secret" }));
    expect(screen.queryByText(/fixture-value/)).toBeNull();
    expect(screen.getByText("Secret hidden")).toBeTruthy();
  });

  it("re-hides on blur (secretEpoch bump) without another backend call", async () => {
    const { rerender } = render(pane());
    await userEvent.click(screen.getByRole("button", { name: "Reveal secret" }));
    await waitFor(() => expect(screen.getByText(/fixture-value/)).toBeTruthy());
    revealSecret.mockClear();

    // Parent bumps the epoch when the window loses focus.
    rerender(pane(1));

    expect(screen.queryByText(/fixture-value/)).toBeNull();
    expect(screen.getByText("Secret hidden")).toBeTruthy();
    expect(revealSecret).not.toHaveBeenCalled();
  });
});
