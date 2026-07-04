import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { TestProviders } from "@/test/test-providers";
import { SourcesPage } from "./SourcesPage";

describe("SourcesPage", () => {
  it("renders Sources tabs", async () => {
    render(
      <TestProviders>
        <SourcesPage />
      </TestProviders>,
    );

    expect(
      await screen.findByRole("heading", { name: /sources/i, level: 2 }),
    ).toBeInTheDocument();
    expect(screen.getAllByRole("tab").map((tab) => tab.textContent)).toEqual([
      "Qobuz Favorites",
      "Qobuz Url",
      "Torrent",
    ]);
    expect(
      screen.getByRole("tab", { name: /qobuz favorites/i }),
    ).toHaveAttribute("aria-selected", "true");
  });

  it("shows magnet and torrent file sections on the Torrent tab", async () => {
    const user = userEvent.setup();
    render(
      <TestProviders>
        <SourcesPage />
      </TestProviders>,
    );

    await user.click(await screen.findByRole("tab", { name: /torrent/i }));

    expect(
      await screen.findByRole("heading", { name: /magnet link/i, level: 3 }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("heading", { name: /\.torrent file/i, level: 3 }),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: /add torrent/i }),
    ).not.toBeInTheDocument();
  });

  it("does not repeat section names as visible torrent input labels", async () => {
    const user = userEvent.setup();
    render(
      <TestProviders>
        <SourcesPage />
      </TestProviders>,
    );

    await user.click(await screen.findByRole("tab", { name: /torrent/i }));
    await screen.findByRole("heading", { name: /magnet link/i, level: 3 });

    expect(screen.getAllByText(/^Magnet link$/i)).toHaveLength(1);
    expect(screen.getAllByText(/^\.torrent file$/i)).toHaveLength(1);
    expect(
      screen.getByRole("textbox", { name: /magnet link/i }),
    ).toBeInTheDocument();
  });

  it("shows Qobuz favorites on the default tab", async () => {
    render(
      <TestProviders>
        <SourcesPage />
      </TestProviders>,
    );

    expect(await screen.findByText("In Lib Album")).toBeInTheDocument();
  });

  it("shows the Qobuz URL panel on the Qobuz Url tab", async () => {
    const user = userEvent.setup();
    render(
      <TestProviders>
        <SourcesPage />
      </TestProviders>,
    );

    await user.click(await screen.findByRole("tab", { name: /qobuz url/i }));

    expect(
      await screen.findByRole("textbox", { name: /qobuz album url/i }),
    ).toBeInTheDocument();
  });
});
