import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { http, HttpResponse } from "msw";
import { MemoryRouter } from "react-router-dom";
import { describe, expect, it } from "vitest";
import { LibraryPage } from "@/features/library/LibraryPage";
import { ToastStateProvider } from "@/hooks/toast-provider";
import { server } from "@/test/msw/server";

function renderPage() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <ToastStateProvider>
        <MemoryRouter>
          <LibraryPage />
        </MemoryRouter>
      </ToastStateProvider>
    </QueryClientProvider>,
  );
}

describe("LibraryPage", () => {
  it("renders album list from API", async () => {
    renderPage();
    expect(await screen.findByText("Local Album")).toBeInTheDocument();
    expect(screen.getByText(/Test Artist/)).toBeInTheDocument();
  });

  it("starts library scan on button click", async () => {
    let scanStarted = false;
    server.use(
      http.post("/api/v1/library/scan", () => {
        scanStarted = true;
        return HttpResponse.json({ scan_id: 2 }, { status: 202 });
      }),
    );
    const user = userEvent.setup();
    renderPage();
    await user.click(await screen.findByRole("button", { name: /rescan library/i }));
    await waitFor(() => expect(scanStarted).toBe(true));
  });

  it("shows No cover placeholders when album has no cover_path", async () => {
    const user = userEvent.setup();
    renderPage();
    await user.click(
      await screen.findByRole("button", { name: /Local Album/i }),
    );
    expect(await screen.findAllByText("No cover")).toHaveLength(2);
  });

  it("does not save tags when opening the editor", async () => {
    let patchCount = 0;
    server.use(
      http.patch("/api/v1/library/tracks/:id", () => {
        patchCount += 1;
        return HttpResponse.json({
          id: 1,
          album_id: 1,
          title: "Track One",
          artist_name: "Test Artist",
          album_title: "Local Album",
          track_number: 1,
          year: 2020,
          disc_number: 1,
          genre: "Pop",
          path: "a/t1.flac",
        });
      }),
    );
    const user = userEvent.setup();
    renderPage();
    await user.click(await screen.findByRole("button", { name: /Local Album/i }));
    await user.click(await screen.findByRole("button", { name: /edit tags/i }));
    await screen.findByLabelText(/^title$/i);
    expect(patchCount).toBe(0);
  });

  it("closes tag editor on Escape", async () => {
    const user = userEvent.setup();
    renderPage();
    await user.click(await screen.findByRole("button", { name: /Local Album/i }));
    await user.click(await screen.findByRole("button", { name: /edit tags/i }));
    await screen.findByLabelText(/^title$/i);
    await user.keyboard("{Escape}");
    await waitFor(() =>
      expect(screen.queryByLabelText(/^title$/i)).not.toBeInTheDocument(),
    );
  });

  it("saves track tags on Enter", async () => {
    let patched = false;
    server.use(
      http.patch("/api/v1/library/tracks/:id", async ({ request }) => {
        patched = true;
        const body = (await request.json()) as { title?: string };
        return HttpResponse.json({
          id: 1,
          album_id: 1,
          title: body.title ?? "Track",
          artist_name: "Test Artist",
          album_title: "Local Album",
          track_number: 1,
          year: 2020,
          disc_number: 1,
          genre: "Pop",
          path: "a/t1.flac",
        });
      }),
    );
    const user = userEvent.setup();
    renderPage();
    await user.click(await screen.findByRole("button", { name: /Local Album/i }));
    await user.click(await screen.findByRole("button", { name: /edit tags/i }));
    const titleInput = await screen.findByLabelText(/title/i);
    await user.clear(titleInput);
    await user.type(titleInput, "Renamed");
    await user.keyboard("{Enter}");
    await waitFor(() => expect(patched).toBe(true));
    await waitFor(() =>
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument(),
    );
  });
});
