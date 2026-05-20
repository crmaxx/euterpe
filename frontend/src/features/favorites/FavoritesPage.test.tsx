import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import { server } from "@/test/msw/server";
import { Toaster } from "@/components/toaster";
import { TestProviders } from "@/test/test-providers";
import { FavoritesPage } from "./FavoritesPage";

function renderFavorites() {
  return render(
    <TestProviders>
      <FavoritesPage />
      <Toaster />
    </TestProviders>,
  );
}

describe("FavoritesPage", () => {
  it("renders mock favorites", async () => {
    renderFavorites();
    expect(await screen.findByText("Test Album")).toBeInTheDocument();
    expect(screen.getAllByText("Test Artist").length).toBeGreaterThanOrEqual(1);
  });

  it("calls sync on Sync now", async () => {
    let synced = false;
    server.use(
      http.post("/api/v1/qobuz/sync", () => {
        synced = true;
        return HttpResponse.json({
          run_id: 2,
          albums_total: 5,
          added: 0,
          removed: 0,
        });
      }),
    );
    const user = userEvent.setup();
    renderFavorites();
    await screen.findByText("Test Album");
    await user.click(screen.getByRole("button", { name: /sync now/i }));
    await waitFor(() => expect(synced).toBe(true));
  });

  it("shows Download when not in library and Re-download when in library", async () => {
    renderFavorites();
    expect(await screen.findByText("In Lib Album")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /^Re-download$/i })).toBeInTheDocument();
    const downloads = screen.getAllByRole("button", { name: /^Download$/i });
    expect(downloads.length).toBeGreaterThanOrEqual(1);
  });

  it("locks row download until job completes", async () => {
    const qobuzId = 393908828;
    let jobStatus = "running";
    server.use(
      http.get("/api/v1/downloads", () =>
        HttpResponse.json({
          items: [
            {
              id: 1,
              status: jobStatus,
              job_type: "album",
              qobuz_id: qobuzId,
              quality: 6,
              progress_pct: jobStatus === "completed" ? 100 : 10,
              created_at: "2026-01-01",
              updated_at: "2026-01-01",
            },
          ],
          next_cursor: null,
          has_more: false,
        }),
      ),
    );
    const user = userEvent.setup();
    renderFavorites();
    const btn = await screen.findByRole("button", { name: /downloading/i });
    expect(btn).toBeDisabled();
    await user.click(btn);
    expect(btn).toBeDisabled();
    jobStatus = "completed";
    await waitFor(
      () =>
        expect(
          screen.getByRole("button", { name: /^Download$/i }),
        ).toBeEnabled(),
      { timeout: 5000 },
    );
  });

});
