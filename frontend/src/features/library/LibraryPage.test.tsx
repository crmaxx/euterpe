import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { MemoryRouter } from "react-router-dom";
import { describe, expect, it } from "vitest";
import { LibraryPage } from "@/features/library/LibraryPage";
import { TestProviders } from "@/test/test-providers";
import { server } from "@/test/msw/server";

function renderPage() {
  return render(
    <TestProviders>
      <MemoryRouter>
        <LibraryPage />
      </MemoryRouter>
    </TestProviders>,
  );
}

describe("LibraryPage", () => {
  it("renders album list from API", async () => {
    renderPage();
    expect(await screen.findByText("Local Album")).toBeInTheDocument();
    expect(screen.getByText(/Test Artist/)).toBeInTheDocument();
  });

  it("starts index rebuild on button click", async () => {
    let scanStarted = false;
    server.use(
      http.post("/api/v1/library/scan", () => {
        scanStarted = true;
        return HttpResponse.json({ scan_id: 2 }, { status: 202 });
      }),
    );
    const user = userEvent.setup();
    renderPage();
    await user.click(await screen.findByRole("button", { name: /rebuild index/i }));
    await waitFor(() => expect(scanStarted).toBe(true));
  });

  it("shows cancel scan while running", async () => {
    let cancelled = false;
    server.use(
      http.get("/api/v1/library/scan/latest", () =>
        HttpResponse.json({
          run: {
            id: 9,
            status: "running",
            files_seen: 2,
            files_processed: 1,
            files_indexed: 0,
            files_total: 0,
            started_at: "2026-01-01T00:00:00Z",
            finished_at: null,
          },
        }),
      ),
      http.delete("/api/v1/library/scan/9", () => {
        cancelled = true;
        return new HttpResponse(null, { status: 204 });
      }),
    );
    const user = userEvent.setup();
    renderPage();
    await user.click(
      await screen.findByRole("button", { name: /cancel scan/i }),
    );
    await waitFor(() => expect(cancelled).toBe(true));
  });

  it("repair folder passes root query", async () => {
    let scanRoot: string | null = null;
    server.use(
      http.post("/api/v1/library/scan", ({ request }) => {
        scanRoot = new URL(request.url).searchParams.get("root");
        return HttpResponse.json({ scan_id: 3 }, { status: 202 });
      }),
    );
    const user = userEvent.setup();
    renderPage();
    await user.click(await screen.findByRole("button", { name: /Local Album/i }));
    await user.click(
      await screen.findByRole("button", { name: /choose album action/i }),
    );
    await user.click(
      await screen.findByRole("menuitem", { name: /repair folder/i }),
    );
    await user.click(
      await screen.findByRole("button", { name: /run: repair folder/i }),
    );
    await waitFor(() =>
      expect(scanRoot).toBe("Test Artist/Local Album"),
    );
  });

  it("shows No cover when cover_path is absent and cover endpoint returns 404", async () => {
    server.use(
      http.get("/api/v1/library/albums/:id/cover", () =>
        new HttpResponse(null, { status: 404 }),
      ),
    );
    const user = userEvent.setup();
    renderPage();
    await user.click(
      await screen.findByRole("button", { name: /Local Album/i }),
    );
    await waitFor(() =>
      expect(screen.getAllByTestId("album-cover-placeholder")).toHaveLength(2),
    );
  });

  it("opens cover file picker when clicking album art", async () => {
    const user = userEvent.setup();
    renderPage();
    await user.click(
      await screen.findByRole("button", { name: /Local Album/i }),
    );
    const input = await screen.findByTestId("album-cover-file-input");
    expect(input).toBeInTheDocument();
    expect(screen.getByTitle("Replace cover")).toBeInTheDocument();
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
