import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { Toaster } from "@/components/toaster";
import { ToastStateProvider } from "@/hooks/toast-provider";
import { SettingsPage } from "./SettingsPage";

function renderSettings() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <ToastStateProvider>
        <SettingsPage />
        <Toaster />
      </ToastStateProvider>
    </QueryClientProvider>,
  );
}

describe("SettingsPage", () => {
  it("shows validation error when token empty", async () => {
    const user = userEvent.setup();
    renderSettings();
    await user.click(screen.getByRole("button", { name: /test connection/i }));
    expect(await screen.findByText(/validation error/i)).toBeInTheDocument();
  });

  it("shows success toast on test login", async () => {
    const user = userEvent.setup();
    renderSettings();
    await user.type(screen.getByLabelText(/user id/i), "123");
    await user.type(screen.getByLabelText(/auth token/i), "good-token");
    await user.click(screen.getByRole("button", { name: /test connection/i }));
    await waitFor(() => {
      expect(screen.getByText(/connection ok/i)).toBeInTheDocument();
    });
  });
});
