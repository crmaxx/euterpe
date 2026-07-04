import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { MemoryRouter } from "react-router-dom";
import { describe, expect, it, vi } from "vitest";
import { Toaster } from "@/components/toaster";
import { server } from "@/test/msw/server";
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
  it("renders Settings tabs with General selected by default", async () => {
    renderSettings();

    await screen.findByRole("tab", { name: /^torrent$/i });
    const tabs = await screen.findAllByRole("tab");
    expect(tabs.map((tab) => tab.textContent)).toEqual([
      "General",
      "Scheduled favorites sync",
      "Integrations",
      "Convert to FLAC",
      "Library scan workers",
      "Library storage",
      "Downloads",
      "Torrent",
    ]);
    expect(screen.getByRole("tab", { name: /^general$/i })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    for (const tab of tabs) {
      const panelId = tab.getAttribute("aria-controls");
      expect(panelId).toBeTruthy();
      expect(document.getElementById(panelId ?? "")).toHaveAttribute(
        "role",
        "tabpanel",
      );
    }
  });

  it("shows requested content in each Settings tab", async () => {
    const user = userEvent.setup();

    renderSettings();

    expect(
      await screen.findByRole("heading", { name: /appearance/i, level: 3 }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("heading", { name: /language/i, level: 3 }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("heading", { name: /qobuz account/i, level: 3 }),
    ).toBeInTheDocument();

    await user.click(screen.getByRole("tab", { name: /scheduled favorites sync/i }));
    expect(
      await screen.findByRole("button", { name: /^save schedule$/i }),
    ).toBeInTheDocument();

    await user.click(screen.getByRole("tab", { name: /^integrations$/i }));
    expect(
      await screen.findByRole("heading", { name: /^integrations$/i, level: 3 }),
    ).toBeInTheDocument();

    await user.click(screen.getByRole("tab", { name: /convert to flac/i }));
    expect(
      await screen.findByRole("heading", { name: /convert to flac/i, level: 3 }),
    ).toBeInTheDocument();

    await user.click(screen.getByRole("tab", { name: /library scan workers/i }));
    expect(
      await screen.findByRole("heading", {
        name: /library scan workers/i,
        level: 3,
      }),
    ).toBeInTheDocument();

    await user.click(screen.getByRole("tab", { name: /library storage/i }));
    const storagePanel = await screen.findByRole("tabpanel", {
      name: /library storage/i,
    });
    expect(
      await within(storagePanel).findByRole("heading", {
        name: /library storage/i,
        level: 3,
      }),
    ).toBeInTheDocument();
    expect(within(storagePanel).getAllByText("local:/music").length).toBeGreaterThan(0);

    await user.click(screen.getByRole("tab", { name: /^downloads$/i }));
    expect(screen.getByText(/default quality/i)).toBeInTheDocument();
    expect(screen.getByText(/download concurrency/i)).toBeInTheDocument();

    await user.click(screen.getByRole("tab", { name: /^torrent$/i }));
    expect(
      await screen.findByRole("heading", { name: /^torrent$/i, level: 3 }),
    ).toBeInTheDocument();
  });

  it("hides the Torrent settings tab when torrent support is not configured", async () => {
    server.use(
      http.get("/api/v1/server/info", () =>
        HttpResponse.json({
          version: "0.1.0",
          library_storage: {
            kind: "local",
            path: "/music",
            watch_status: { state: "disabled" },
          },
          credentials_configured: true,
          admin_auth_required: false,
          torrent_incoming_dir: null,
          ui: { theme: "system", locale: "en", default_quality: 6 },
        }),
      ),
    );

    renderSettings();

    await screen.findByRole("tab", { name: /^general$/i });
    expect(screen.queryByRole("tab", { name: /^torrent$/i })).not.toBeInTheDocument();
  });

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

    await user.click(await screen.findByRole("tab", { name: /scheduled favorites sync/i }));
    expect(
      await screen.findByRole("heading", {
        name: /scheduled favorites sync/i,
        level: 3,
      }),
    ).toBeInTheDocument();
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

    await user.click(await screen.findByRole("tab", { name: /scheduled favorites sync/i }));
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

  it("keeps unsaved scheduled sync edits when switching Settings tabs", async () => {
    const user = userEvent.setup();

    renderSettings();

    await user.click(await screen.findByRole("tab", { name: /scheduled favorites sync/i }));
    const cron = await screen.findByLabelText(/cron expression/i);
    await user.clear(cron);
    await user.type(cron, "15 4 * * *");

    await user.click(screen.getByRole("tab", { name: /^integrations$/i }));
    await screen.findByRole("heading", { name: /^integrations$/i, level: 3 });
    await user.click(screen.getByRole("tab", { name: /scheduled favorites sync/i }));

    expect(await screen.findByLabelText(/cron expression/i)).toHaveValue(
      "15 4 * * *",
    );
  });
});
