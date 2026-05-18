import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { JobProgressEvent } from "@/api/client";
import { ToastStateProvider } from "@/hooks/use-toast";
import { QueuePage } from "./QueuePage";

class MockEventSource {
  static instances: MockEventSource[] = [];
  onmessage: ((ev: MessageEvent) => void) | null = null;
  private listeners = new Map<string, (ev: MessageEvent) => void>();

  constructor(public url: string) {
    MockEventSource.instances.push(this);
  }

  addEventListener(type: string, fn: (ev: MessageEvent) => void) {
    this.listeners.set(type, fn);
  }

  emit(type: string, data: string) {
    const fn = this.listeners.get(type);
    fn?.({ data } as MessageEvent);
  }

  close() {}
}

describe("QueuePage", () => {
  it("updates progress bar from SSE job_progress", async () => {
    vi.stubGlobal("EventSource", MockEventSource as unknown as typeof EventSource);

    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    render(
      <QueryClientProvider client={qc}>
        <ToastStateProvider>
          <QueuePage />
        </ToastStateProvider>
      </QueryClientProvider>,
    );

    await screen.findByText(/10%/);

    const ev: JobProgressEvent = { id: 1, progress_pct: 50 };
    await act(async () => {
      MockEventSource.instances[0]?.emit("job_progress", JSON.stringify(ev));
    });

    await waitFor(() => {
      expect(screen.getByLabelText("Progress 50%")).toBeInTheDocument();
    });

    vi.unstubAllGlobals();
  });
});
