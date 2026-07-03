import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router-dom";
import { describe, expect, it, vi } from "vitest";
import { Toaster } from "@/components/toaster";
import { TestProviders } from "@/test/test-providers";
import { SettingsPage } from "./SettingsPage";

function renderSettings(initialEntries = ["/settings"]) {
  return render(
    <TestProviders>
      <MemoryRouter initialEntries={initialEntries}>
        <SettingsPage />
        <Toaster />
      </MemoryRouter>
    </TestProviders>,
  );
}

describe("SettingsPage", () => {
  it("shows connect Qobuz when not connected", async () => {
    renderSettings();
    expect(
      await screen.findByRole("button", { name: /connect qobuz/i }),
    ).toBeInTheDocument();
    expect(screen.getByText(/not signed in/i)).toBeInTheDocument();
  });

  it("calls oauth start when connect clicked", async () => {
    const user = userEvent.setup();
    const fetchSpy = vi.spyOn(globalThis, "fetch");

    renderSettings();
    await user.click(await screen.findByRole("button", { name: /connect qobuz/i }));

    await waitFor(() => {
      expect(fetchSpy).toHaveBeenCalledWith(
        expect.stringContaining("/api/v1/qobuz/oauth/start"),
        expect.any(Object),
      );
    });

    expect(window.location.href).toContain("qobuz.com/signin/oauth");

    fetchSpy.mockRestore();
  });

  it("shows connected toast after oauth callback redirect", async () => {
    renderSettings(["/settings?qobuz=connected&account_id=1"]);
    expect(await screen.findByText(/qobuz connected/i)).toBeInTheDocument();
  });

  it("shows and saves Qobuz scheduled sync settings", async () => {
    const user = userEvent.setup();
    const fetchSpy = vi.spyOn(globalThis, "fetch");

    renderSettings();

    expect(await screen.findByText(/scheduled favorites sync/i)).toBeInTheDocument();
    await user.click(screen.getByLabelText(/enable scheduled sync/i));
    await user.clear(screen.getByLabelText(/cron expression/i));
    await user.type(screen.getByLabelText(/cron expression/i), "0 3 * * *");
    await user.click(screen.getByLabelText(/auto-download new favorites/i));
    await user.click(screen.getByRole("button", { name: /^save schedule$/i }));

    await waitFor(() => {
      expect(fetchSpy).toHaveBeenCalledWith(
        expect.stringContaining("/api/v1/settings/qobuz-scheduled-sync"),
        expect.objectContaining({ method: "PATCH" }),
      );
    });

    fetchSpy.mockRestore();
  });

  it("runs Qobuz scheduled sync now from settings", async () => {
    const user = userEvent.setup();
    const fetchSpy = vi.spyOn(globalThis, "fetch");

    renderSettings();

    await user.click(await screen.findByRole("button", { name: /run now/i }));

    await waitFor(() => {
      expect(fetchSpy).toHaveBeenCalledWith(
        expect.stringContaining("/api/v1/settings/qobuz-scheduled-sync/run"),
        expect.objectContaining({ method: "POST" }),
      );
    });
    expect(
      await screen.findByText(/auto-download is off, so no download jobs were queued/i),
    ).toBeInTheDocument();
    expect(
      await screen.findByText(/settings_run_now: success .* new 0/i),
    ).toBeInTheDocument();

    fetchSpy.mockRestore();
  });
});
