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
});
