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
    expect(screen.getByRole("tab", { name: /torrent/i })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /qobuz url/i })).toBeInTheDocument();
    expect(
      screen.getByRole("tab", { name: /qobuz favorites/i }),
    ).toBeInTheDocument();
  });

  it("shows magnet and torrent file sections on the Torrent tab", async () => {
    render(
      <TestProviders>
        <SourcesPage />
      </TestProviders>,
    );

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
    render(
      <TestProviders>
        <SourcesPage />
      </TestProviders>,
    );

    await screen.findByRole("heading", { name: /magnet link/i, level: 3 });

    expect(screen.getAllByText(/^Magnet link$/i)).toHaveLength(1);
    expect(screen.getAllByText(/^\.torrent file$/i)).toHaveLength(1);
    expect(
      screen.getByRole("textbox", { name: /magnet link/i }),
    ).toBeInTheDocument();
  });

  it("moves Qobuz favorites into a Sources tab", async () => {
    const user = userEvent.setup();
    render(
      <TestProviders>
        <SourcesPage />
      </TestProviders>,
    );

    await user.click(
      await screen.findByRole("tab", { name: /qobuz favorites/i }),
    );

    expect(await screen.findByText("Test Album")).toBeInTheDocument();
  });
});
