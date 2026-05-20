import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { TestProviders } from "@/test/test-providers";
import { SourcesPage } from "./SourcesPage";

describe("SourcesPage", () => {
  it("renders Qobuz and torrent sections", async () => {
    render(
      <TestProviders>
        <SourcesPage />
      </TestProviders>,
    );

    expect(
      await screen.findByRole("heading", { name: /sources/i, level: 2 }),
    ).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: /qobuz/i, level: 3 })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: /torrent/i, level: 3 })).toBeInTheDocument();
  });
});
