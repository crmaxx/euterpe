import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeAll, describe, expect, it, vi } from "vitest";
import { http, HttpResponse } from "msw";
import type { JobProgressEvent } from "@/api/client";
import { TestProviders } from "@/test/test-providers";
import { server } from "@/test/msw/server";
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

beforeAll(() => {
  Object.defineProperties(HTMLElement.prototype, {
    hasPointerCapture: { value: () => false },
    setPointerCapture: { value: () => undefined },
    releasePointerCapture: { value: () => undefined },
    scrollIntoView: { value: () => undefined },
  });
});

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

  it("shows global queue actions", async () => {
    vi.stubGlobal("EventSource", MockEventSource as unknown as typeof EventSource);
    vi.stubGlobal("confirm", vi.fn(() => true));

    render(
      <TestProviders>
        <QueuePage />
      </TestProviders>,
    );

    await screen.findByRole("button", { name: /clear history/i });
    expect(screen.getByRole("button", { name: /retry all/i })).toBeInTheDocument();
    expect(screen.getByRole("combobox", { name: /filter by status/i })).toBeInTheDocument();
  });

  it("filters jobs by status", async () => {
    vi.stubGlobal("EventSource", MockEventSource as unknown as typeof EventSource);
    const user = userEvent.setup();

    render(
      <TestProviders>
        <QueuePage />
      </TestProviders>,
    );

    await screen.findByText("Artist — Album");

    await user.click(screen.getByRole("combobox", { name: /filter by status/i }));
    await user.click(await screen.findByRole("option", { name: "Failed" }));

    await screen.findByText("Retry — Needed");
    await waitFor(() => {
      expect(screen.queryByText("Artist — Album")).not.toBeInTheDocument();
      expect(screen.queryByText("Other — Done")).not.toBeInTheDocument();
    });
  });

  it("purges completed jobs on Clear history confirm", async () => {
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
    expect(confirm).toHaveBeenCalledWith(
      "Remove all completed jobs from the list? Failed and cancelled jobs will be kept.",
    );
  });

  it("retries all failed downloads from the toolbar", async () => {
    vi.stubGlobal("EventSource", MockEventSource as unknown as typeof EventSource);
    const user = userEvent.setup();
    const retryAll = vi.fn();
    server.use(
      http.post("/api/v1/downloads/retry", () => {
        retryAll();
        return HttpResponse.json({ retried: 1 });
      }),
    );

    render(
      <TestProviders>
        <QueuePage />
      </TestProviders>,
    );

    await user.click(await screen.findByRole("button", { name: /retry all/i }));

    await waitFor(() => expect(retryAll).toHaveBeenCalledTimes(1));
  });
});
