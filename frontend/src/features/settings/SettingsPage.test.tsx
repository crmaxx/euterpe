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
});
