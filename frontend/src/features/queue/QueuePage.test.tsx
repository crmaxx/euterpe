import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { JobProgressEvent } from "@/api/client";
import { TestProviders } from "@/test/test-providers";
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

    render(
      <TestProviders>
        <QueuePage />
      </TestProviders>,
    );

    await screen.findByText(/10%/);

    const ev: JobProgressEvent = {
      id: 1,
      progress_pct: 50,
      download_speed_bps: 1_048_576,
    };
    await act(async () => {
      MockEventSource.instances[0]?.emit("job_progress", JSON.stringify(ev));
    });

    await waitFor(() => {
      expect(screen.getByLabelText("Progress 50%")).toBeInTheDocument();
    });

  });

  it("shows Clear history when terminal jobs exist", async () => {
    vi.stubGlobal("EventSource", MockEventSource as unknown as typeof EventSource);
    vi.stubGlobal("confirm", vi.fn(() => true));

    render(
      <TestProviders>
        <QueuePage />
      </TestProviders>,
    );

    await screen.findByRole("button", { name: /clear history/i });
  });

  it("purges finished jobs on Clear history confirm", async () => {
    vi.stubGlobal("EventSource", MockEventSource as unknown as typeof EventSource);
    const confirm = vi.fn(() => true);
    vi.stubGlobal("confirm", confirm);
    const user = userEvent.setup();

    render(
      <TestProviders>
        <QueuePage />
      </TestProviders>,
    );

    await user.click(await screen.findByRole("button", { name: /clear history/i }));
    expect(confirm).toHaveBeenCalled();
  });
});
