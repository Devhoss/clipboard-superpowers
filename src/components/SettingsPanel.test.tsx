import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { SettingsPanel } from "./SettingsPanel";
import type { AppSettings } from "@/lib/types";

const updateSettings = vi.fn(async (s: AppSettings) => s);

vi.mock("@/lib/api", () => ({
  api: {
    getStats: vi.fn(async () => ({
      total: 12,
      pinned: 1,
      secrets: 3,
      db_bytes: 4096,
    })),
    updateSettings: (s: AppSettings) => updateSettings(s),
    clearHistory: vi.fn(async () => 0),
  },
}));

const base: AppSettings = {
  hotkey: "CommandOrControl+Shift+V",
  actions_hotkey: "CommandOrControl+K",
  max_items: 500,
  launch_on_login: false,
  capture_text: true,
  capture_images: true,
  hide_on_blur: true,
  skip_secrets: false,
  capture_files: false,
};

const noop = () => {};

afterEach(() => {
  cleanup();
  updateSettings.mockClear();
});

describe("skip-secrets toggle is arm-to-confirm", () => {
  it("names the row count at rest and warns on the first tap", async () => {
    render(
      <SettingsPanel
        initial={base}
        onSaved={noop}
        onCleared={noop}
      />,
    );

    const toggle = screen.getByRole("switch", { name: "Skip passwords & OTPs" });
    // getStats is async, so the count only appears once stats land.
    expect(await screen.findByText(/keeps 3 stored secrets/)).toBeTruthy();

    await userEvent.click(toggle);

    // First tap arms only: still off, and the count is now on screen as a
    // consequence being offered.
    expect(toggle.getAttribute("aria-checked")).toBe("false");
    expect(screen.getByText(/Tap again, then Save, to delete 3 secrets for good/))
      .toBeTruthy();
    expect(updateSettings).not.toHaveBeenCalled();
  });

  it("does not enable after a single tap, so no purge can be saved", async () => {
    render(<SettingsPanel initial={base} onSaved={noop} onCleared={noop} />);

    await userEvent.click(screen.getByRole("switch", { name: "Skip passwords & OTPs" }));

    // Save stays disabled: the draft was not mutated, so nothing is dirty and
    // update_settings — the command that purges — is never reached.
    expect(screen.getByRole("button", { name: "Save settings" })).toHaveProperty(
      "disabled",
      true,
    );
    expect(updateSettings).not.toHaveBeenCalled();
  });

  it("enables only after a second tap, and then saves the purge", async () => {
    render(<SettingsPanel initial={base} onSaved={noop} onCleared={noop} />);
    const toggle = screen.getByRole("switch", { name: "Skip passwords & OTPs" });

    await userEvent.click(toggle);
    await userEvent.click(toggle);
    expect(toggle.getAttribute("aria-checked")).toBe("true");

    const save = screen.getByRole("button", { name: "Save settings" });
    await waitFor(() => expect(save).toHaveProperty("disabled", false));
    await userEvent.click(save);

    await waitFor(() => expect(updateSettings).toHaveBeenCalledTimes(1));
    expect(updateSettings.mock.calls[0][0].skip_secrets).toBe(true);
  });

  it("turning it back off is a single tap and deletes nothing", async () => {
    render(
      <SettingsPanel
        initial={{ ...base, skip_secrets: true }}
        onSaved={noop}
        onCleared={noop}
      />,
    );

    const toggle = screen.getByRole("switch", { name: "Skip passwords & OTPs" });
    expect(screen.getByText(/On: new secrets are skipped/)).toBeTruthy();
    await userEvent.click(toggle);

    expect(toggle.getAttribute("aria-checked")).toBe("false");
    // Disarming is the reverse of arming: no second tap required, because
    // switching off never removes rows.
    expect(await screen.findByText(/keeps 3 stored secrets/)).toBeTruthy();
  });
});
