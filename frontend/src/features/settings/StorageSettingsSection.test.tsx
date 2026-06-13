import { focusManager } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import { Toaster } from "@/components/toaster";
import { StorageSettingsSection } from "@/features/settings/StorageSettingsSection";
import { server } from "@/test/msw/server";
import { TestProviders } from "@/test/test-providers";

function renderSection() {
  return render(
    <TestProviders>
      <StorageSettingsSection />
      <Toaster />
    </TestProviders>,
  );
}

describe("StorageSettingsSection", () => {
  it("shows full-scan toast when storage kind changes", async () => {
    server.use(
      http.patch("/api/v1/settings/storage", async ({ request }) => {
        const body = (await request.json()) as { library: { kind: string } };
        return HttpResponse.json({
          settings: {
            library: {
              ...(body.library as Record<string, unknown>),
              watch_status: { state: "disabled" },
            },
            presets: [],
          },
          recommend_full_scan: true,
          storage_migration_hint:
            "Library storage switched from local disk to SMB. Run a full library scan to rebuild the index.",
        });
      }),
    );

    const user = userEvent.setup();
    renderSection();
    await screen.findByText(/library storage/i);
    await user.click(screen.getByRole("button", { name: /^save$/i }));
    expect(
      await screen.findByText(/full library scan recommended/i),
    ).toBeInTheDocument();
    await waitFor(() => {
      expect(screen.getByText(/switched from local disk to smb/i)).toBeInTheDocument();
    });
  });

  it("sends explicit SMB credential clears when saved fields are emptied", async () => {
    const patchBodies: unknown[] = [];
    server.use(
      http.get("/api/v1/settings/storage", () =>
        HttpResponse.json({
          settings: {
            library: {
              kind: "smb",
              host: "nas.local",
              port: 445,
              share: "music",
              path: "library",
              username: "music-user",
              workgroup: "WORKGROUP",
              password_configured: true,
              watch_status: { state: "disabled" },
            },
            presets: [],
          },
        }),
      ),
      http.patch("/api/v1/settings/storage", async ({ request }) => {
        const body = await request.json();
        patchBodies.push(body);
        return HttpResponse.json({
          settings: {
            library: {
              ...(body as { library: Record<string, unknown> }).library,
              password_configured: false,
              watch_status: { state: "disabled" },
            },
            presets: [],
          },
        });
      }),
    );

    const user = userEvent.setup();
    renderSection();
    await screen.findByDisplayValue("music-user");

    await user.clear(screen.getByLabelText(/username/i));
    await user.clear(screen.getByLabelText(/workgroup/i));
    await user.click(screen.getByLabelText(/clear stored password/i));
    await user.click(screen.getByRole("button", { name: /^save$/i }));

    await waitFor(() => expect(patchBodies).toHaveLength(1));
    expect(patchBodies[0]).toMatchObject({
      library: {
        kind: "smb",
        username: null,
        workgroup: null,
        password: null,
      },
    });
  });

  it("browses the draft location from the form instead of the last saved location", async () => {
    const browseBodies: unknown[] = [];
    server.use(
      http.get("/api/v1/settings/storage", () =>
        HttpResponse.json({
          settings: {
            library: {
              kind: "smb",
              host: "saved.local",
              port: 445,
              share: "saved",
              path: "library",
              password_configured: false,
              watch_status: { state: "disabled" },
            },
            presets: [],
          },
        }),
      ),
      http.post("/api/v1/settings/storage/browse", async ({ request }) => {
        const body = await request.json();
        browseBodies.push(body);
        return HttpResponse.json({
          entries: [{ name: "draft-only", path: "draft-only", is_dir: true }],
        });
      }),
    );

    const user = userEvent.setup();
    renderSection();
    const location = await screen.findByLabelText(/network share location/i);
    await user.clear(location);
    await user.type(location, "smb://draft.local/draft/new-root");
    await user.click(screen.getByTitle(/reload/i));

    await waitFor(() => {
      expect(browseBodies).toContainEqual(
        expect.objectContaining({
          location: expect.objectContaining({
            kind: "smb",
            host: "draft.local",
            share: "draft",
            path: "new-root",
          }),
          path: "",
        }),
      );
    });
  });

  it("does not wipe unsaved edits when settings refetch returns the same saved identity", async () => {
    let settingsReads = 0;
    server.use(
      http.get("/api/v1/settings/storage", () => {
        settingsReads += 1;
        return HttpResponse.json({
          settings: {
            library: {
              kind: "smb",
              host: "nas.local",
              port: 445,
              share: "music",
              path: "library",
              username: "saved-user",
              password_configured: false,
              watch_status: { state: "disabled" },
            },
            presets: [],
          },
        });
      }),
      http.post("/api/v1/settings/storage/browse", () =>
        HttpResponse.json({ entries: [] }),
      ),
    );

    const user = userEvent.setup();
    renderSection();
    const username = await screen.findByLabelText(/username/i);
    await user.clear(username);
    await user.type(username, "draft-user");

    focusManager.setFocused(false);
    focusManager.setFocused(true);

    await waitFor(() => expect(settingsReads).toBeGreaterThan(1));
    expect(screen.getByDisplayValue("draft-user")).toBeInTheDocument();
  });
});
