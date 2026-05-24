import { render, screen } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { describe, expect, it } from "vitest";
import { TestProviders } from "@/test/test-providers";
import { AppLayout } from "./AppLayout";

describe("AppLayout", () => {
  it("keeps Favorites out of the main navigation", async () => {
    render(
      <TestProviders>
        <MemoryRouter initialEntries={["/sources"]}>
          <Routes>
            <Route element={<AppLayout />}>
              <Route path="/sources" element={<div>Sources content</div>} />
            </Route>
          </Routes>
        </MemoryRouter>
      </TestProviders>,
    );

    expect(await screen.findByRole("link", { name: /sources/i })).toBeInTheDocument();
    expect(screen.queryByRole("link", { name: /favorites/i })).not.toBeInTheDocument();
  });
});
