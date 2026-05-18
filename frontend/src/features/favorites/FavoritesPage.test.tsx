import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import { server } from "@/test/msw/server";
import { Toaster } from "@/components/toaster";
import { ToastStateProvider } from "@/hooks/toast-provider";
import { FavoritesPage } from "./FavoritesPage";

function renderFavorites() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <ToastStateProvider>
        <FavoritesPage />
        <Toaster />
      </ToastStateProvider>
    </QueryClientProvider>,
  );
}

describe("FavoritesPage", () => {
  it("renders mock favorites", async () => {
    renderFavorites();
    expect(await screen.findByText("Test Album")).toBeInTheDocument();
    expect(screen.getByText("Test Artist")).toBeInTheDocument();
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

  it("queues download by URL", async () => {
    let queued = false;
    server.use(
      http.post("/api/v1/downloads/by-url", async ({ request }) => {
        queued = true;
        const body = (await request.json()) as { url: string; quality: number };
        expect(body.url).toContain("play.qobuz.com");
        expect(body.quality).toBeGreaterThan(0);
        return HttpResponse.json({ job_id: 99 }, { status: 202 });
      }),
    );
    const user = userEvent.setup();
    renderFavorites();
    await screen.findByText("Test Album");
    await user.click(screen.getByRole("button", { name: /download by url/i }));
    const urlField = screen.getByLabelText(/qobuz album url/i);
    const panel = urlField.closest("div.rounded-lg");
    expect(panel).toBeTruthy();
    await user.type(urlField, "https://play.qobuz.com/album/test");
    await user.click(
      within(panel as HTMLElement).getByRole("button", { name: /^download$/i }),
    );
    await waitFor(() => expect(queued).toBe(true));
    expect(await screen.findByText("Download queued")).toBeInTheDocument();
  });
});
