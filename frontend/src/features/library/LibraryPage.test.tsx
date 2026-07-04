import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { MemoryRouter } from "react-router-dom";
import { beforeAll, describe, expect, it } from "vitest";
import { LibraryPage } from "@/features/library/LibraryPage";
import { TestProviders } from "@/test/test-providers";
import { server } from "@/test/msw/server";

function renderPage() {
  return render(
    <TestProviders>
      <MemoryRouter>
        <LibraryPage />
      </MemoryRouter>
    </TestProviders>,
  );
}

type MockAlbum = {
  id: number;
  title: string;
  artist_name: string;
  year: number | null;
  created_at: string;
  track_count: number;
  cover_path: string | null;
  has_cue_files: boolean;
};

const sortableAlbums: MockAlbum[] = [
  {
    id: 1,
    title: "Zulu",
    artist_name: "Alpha Artist",
    year: 2020,
    created_at: "2026-01-01 00:00:00",
    track_count: 1,
    cover_path: null,
    has_cue_files: false,
  },
  {
    id: 2,
    title: "Alpha",
    artist_name: "Zulu Artist",
    year: 2024,
    created_at: "2026-01-03 00:00:00",
    track_count: 1,
    cover_path: null,
    has_cue_files: false,
  },
  {
    id: 3,
    title: "Middle",
    artist_name: "Middle Artist",
    year: null,
    created_at: "2026-01-02 00:00:00",
    track_count: 1,
    cover_path: null,
    has_cue_files: false,
  },
];

type LibraryAlbumsRequest = {
  sort: string;
  order: string;
  q: string | null;
  cursor: string | null;
};

function mockAlbumYearSortValue(year: number | null, order: string) {
  if (year != null) {
    return year;
  }
  return order === "desc" ? Number.MIN_SAFE_INTEGER : Number.MAX_SAFE_INTEGER;
}

function compareMockAlbums(
  left: MockAlbum,
  right: MockAlbum,
  sort: string,
  order: string,
) {
  let primary: number;
  if (sort === "artist") {
    primary = left.artist_name.localeCompare(right.artist_name);
  } else if (sort === "album_date") {
    primary =
      mockAlbumYearSortValue(left.year, order) -
      mockAlbumYearSortValue(right.year, order);
  } else if (sort === "date_added") {
    primary = left.created_at.localeCompare(right.created_at);
  } else {
    primary = left.title.localeCompare(right.title);
  }
  const direction = order === "desc" ? -1 : 1;
  return primary === 0 ? left.id - right.id : primary * direction;
}

function comparePositionBefore(left: HTMLElement, right: HTMLElement) {
  expect(
    left.compareDocumentPosition(right) & Node.DOCUMENT_POSITION_FOLLOWING,
  ).not.toBe(0);
}

function mockSortableAlbums(requests: LibraryAlbumsRequest[] = []) {
  server.use(
    http.get("/api/v1/library/albums", ({ request }) => {
      const params = new URL(request.url).searchParams;
      const sort = params.get("sort") ?? "title";
      const order = params.get("order") ?? "asc";
      const q = params.get("q");
      const cursor = params.get("cursor");
      requests.push({ sort, order, q, cursor });

      const filtered = sortableAlbums
        .filter(
          (album) =>
            !q ||
            album.title.toLowerCase().includes(q.toLowerCase()) ||
            album.artist_name.toLowerCase().includes(q.toLowerCase()),
        )
        .sort((left, right) => compareMockAlbums(left, right, sort, order));

      const start = cursor == null ? 0 : Number.parseInt(cursor, 10);
      const page = filtered.slice(start, start + 2);
      const next = start + 2 < filtered.length ? String(start + 2) : null;

      return HttpResponse.json({
        items: page.map(
          ({ id, title, artist_name, year, track_count, cover_path, has_cue_files }) => ({
            id,
            title,
            artist_name,
            year,
            track_count,
            cover_path,
            has_cue_files,
          }),
        ),
        next_cursor: next,
        has_more: next != null,
      });
    }),
  );
}

beforeAll(() => {
  Object.defineProperties(HTMLElement.prototype, {
    hasPointerCapture: { value: () => false },
    setPointerCapture: { value: () => undefined },
    releasePointerCapture: { value: () => undefined },
    scrollIntoView: { value: () => undefined },
  });
});

describe("LibraryPage", () => {
  it("renders album list from API", async () => {
    renderPage();
    expect(await screen.findByText("Local Album")).toBeInTheDocument();
    expect(screen.getByText(/Test Artist/)).toBeInTheDocument();
  });

  it("requests title ascending by default", async () => {
    const requests: LibraryAlbumsRequest[] = [];
    mockSortableAlbums(requests);

    renderPage();

    const alpha = await screen.findByText("Alpha");
    const middle = await screen.findByText("Middle");
    comparePositionBefore(alpha, middle);
    await waitFor(() =>
      expect(requests[0]).toMatchObject({ sort: "title", order: "asc", cursor: null }),
    );
  });

  it("sorts by artist through the Library API params", async () => {
    const requests: LibraryAlbumsRequest[] = [];
    mockSortableAlbums(requests);
    const user = userEvent.setup();
    renderPage();

    await user.click(await screen.findByRole("combobox", { name: /sort by/i }));
    await user.click(await screen.findByRole("option", { name: "Artist" }));

    const zulu = await screen.findByText("Zulu");
    const middle = await screen.findByText("Middle");
    comparePositionBefore(zulu, middle);
    await waitFor(() =>
      expect(requests.at(-1)).toMatchObject({ sort: "artist", order: "asc" }),
    );
  });

  it("sorts by album date descending", async () => {
    const requests: LibraryAlbumsRequest[] = [];
    mockSortableAlbums(requests);
    const user = userEvent.setup();
    renderPage();

    await user.click(await screen.findByRole("combobox", { name: /sort by/i }));
    await user.click(await screen.findByRole("option", { name: "Album date" }));
    await user.click(screen.getByRole("button", { name: /sort ascending/i }));

    const alpha = await screen.findByText("Alpha");
    const zulu = await screen.findByText("Zulu");
    comparePositionBefore(alpha, zulu);
    await waitFor(() =>
      expect(requests.at(-1)).toMatchObject({ sort: "album_date", order: "desc" }),
    );
  });

  it("resets pagination when sort changes after loading more", async () => {
    const requests: LibraryAlbumsRequest[] = [];
    mockSortableAlbums(requests);
    const user = userEvent.setup();
    renderPage();

    await user.click(await screen.findByRole("button", { name: /load more/i }));
    await waitFor(() =>
      expect(requests.some((request) => request.cursor === "2")).toBe(true),
    );

    await user.click(await screen.findByRole("combobox", { name: /sort by/i }));
    await user.click(await screen.findByRole("option", { name: "Date added" }));
    await user.click(screen.getByRole("button", { name: /sort ascending/i }));

    await waitFor(() =>
      expect(requests.at(-1)).toMatchObject({
        sort: "date_added",
        order: "desc",
        cursor: null,
      }),
    );
  });

  it("keeps current sort and order when searching", async () => {
    const requests: LibraryAlbumsRequest[] = [];
    mockSortableAlbums(requests);
    const user = userEvent.setup();
    renderPage();

    await user.click(await screen.findByRole("combobox", { name: /sort by/i }));
    await user.click(await screen.findByRole("option", { name: "Artist" }));
    await user.click(screen.getByRole("button", { name: /sort ascending/i }));
    await user.type(screen.getByLabelText(/search/i), "zulu");

    await waitFor(() =>
      expect(requests.at(-1)).toMatchObject({
        sort: "artist",
        order: "desc",
        q: "zulu",
        cursor: null,
      }),
    );
  });

  it("sorts visible albums by date added descending", async () => {
    const requests: LibraryAlbumsRequest[] = [];
    mockSortableAlbums(requests);
    const user = userEvent.setup();
    renderPage();

    await user.click(await screen.findByRole("combobox", { name: /sort by/i }));
    await user.click(await screen.findByRole("option", { name: "Date added" }));
    await user.click(screen.getByRole("button", { name: /sort ascending/i }));

    const alpha = await screen.findByText("Alpha");
    const middle = await screen.findByText("Middle");
    comparePositionBefore(alpha, middle);
    await waitFor(() =>
      expect(requests.at(-1)).toMatchObject({ sort: "date_added", order: "desc" }),
    );
  });

  it("starts index rebuild on button click", async () => {
    let scanStarted = false;
    server.use(
      http.post("/api/v1/library/scan", () => {
        scanStarted = true;
        return HttpResponse.json({ scan_id: 2 }, { status: 202 });
      }),
    );
    const user = userEvent.setup();
    renderPage();
    await user.click(await screen.findByRole("button", { name: /rebuild index/i }));
    await waitFor(() => expect(scanStarted).toBe(true));
  });

  it("shows cancel scan while running", async () => {
    let cancelled = false;
    server.use(
      http.get("/api/v1/library/scan/latest", () =>
        HttpResponse.json({
          run: {
            id: 9,
            status: "running",
            files_seen: 2,
            files_processed: 1,
            files_indexed: 0,
            files_total: 0,
            started_at: "2026-01-01T00:00:00Z",
            finished_at: null,
          },
        }),
      ),
      http.delete("/api/v1/library/scan/9", () => {
        cancelled = true;
        return new HttpResponse(null, { status: 204 });
      }),
    );
    const user = userEvent.setup();
    renderPage();
    await user.click(
      await screen.findByRole("button", { name: /cancel scan/i }),
    );
    await waitFor(() => expect(cancelled).toBe(true));
  });

  it("repair folder passes root query", async () => {
    let scanRoot: string | null = null;
    server.use(
      http.post("/api/v1/library/scan", ({ request }) => {
        scanRoot = new URL(request.url).searchParams.get("root");
        return HttpResponse.json({ scan_id: 3 }, { status: 202 });
      }),
    );
    const user = userEvent.setup();
    renderPage();
    await user.click(await screen.findByRole("button", { name: /Local Album/i }));
    await user.click(
      await screen.findByRole("button", { name: /choose album action/i }),
    );
    await user.click(
      await screen.findByRole("menuitem", { name: /repair folder/i }),
    );
    await user.click(
      await screen.findByRole("button", { name: /run: repair folder/i }),
    );
    await waitFor(() =>
      expect(scanRoot).toBe("Test Artist/Local Album"),
    );
  });

  it("shows No cover when cover_path is absent and cover endpoint returns 404", async () => {
    server.use(
      http.get("/api/v1/library/albums/:id/cover", () =>
        new HttpResponse(null, { status: 404 }),
      ),
    );
    const user = userEvent.setup();
    renderPage();
    await user.click(
      await screen.findByRole("button", { name: /Local Album/i }),
    );
    await waitFor(() =>
      expect(screen.getAllByTestId("album-cover-placeholder")).toHaveLength(2),
    );
  });

  it("opens cover file picker when clicking album art", async () => {
    const user = userEvent.setup();
    renderPage();
    await user.click(
      await screen.findByRole("button", { name: /Local Album/i }),
    );
    const input = await screen.findByTestId("album-cover-file-input");
    expect(input).toBeInTheDocument();
    expect(screen.getByTitle("Replace cover")).toBeInTheDocument();
  });

  it("does not save tags when opening the editor", async () => {
    let patchCount = 0;
    server.use(
      http.patch("/api/v1/library/tracks/:id", () => {
        patchCount += 1;
        return HttpResponse.json({
          id: 1,
          album_id: 1,
          title: "Track One",
          artist_name: "Test Artist",
          album_title: "Local Album",
          track_number: 1,
          year: 2020,
          disc_number: 1,
          genre: "Pop",
          path: "a/t1.flac",
        });
      }),
    );
    const user = userEvent.setup();
    renderPage();
    await user.click(await screen.findByRole("button", { name: /Local Album/i }));
    await user.click(await screen.findByRole("button", { name: /edit tags/i }));
    await screen.findByLabelText(/^title$/i);
    expect(patchCount).toBe(0);
  });

  it("closes tag editor on Escape", async () => {
    const user = userEvent.setup();
    renderPage();
    await user.click(await screen.findByRole("button", { name: /Local Album/i }));
    await user.click(await screen.findByRole("button", { name: /edit tags/i }));
    await screen.findByLabelText(/^title$/i);
    await user.keyboard("{Escape}");
    await waitFor(() =>
      expect(screen.queryByLabelText(/^title$/i)).not.toBeInTheDocument(),
    );
  });

  it("saves track tags on Enter", async () => {
    let patched = false;
    server.use(
      http.patch("/api/v1/library/tracks/:id", async ({ request }) => {
        patched = true;
        const body = (await request.json()) as { title?: string };
        return HttpResponse.json({
          id: 1,
          album_id: 1,
          title: body.title ?? "Track",
          artist_name: "Test Artist",
          album_title: "Local Album",
          track_number: 1,
          year: 2020,
          disc_number: 1,
          genre: "Pop",
          path: "a/t1.flac",
        });
      }),
    );
    const user = userEvent.setup();
    renderPage();
    await user.click(await screen.findByRole("button", { name: /Local Album/i }));
    await user.click(await screen.findByRole("button", { name: /edit tags/i }));
    const titleInput = await screen.findByLabelText(/title/i);
    await user.clear(titleInput);
    await user.type(titleInput, "Renamed");
    await user.keyboard("{Enter}");
    await waitFor(() => expect(patched).toBe(true));
    await waitFor(() =>
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument(),
    );
  });

  it("opens CUE editor from album actions when album has cue files", async () => {
    const user = userEvent.setup();
    renderPage();
    await user.click(await screen.findByRole("button", { name: /Local Album/i }));
    await user.click(
      await screen.findByRole("button", { name: /choose album action/i }),
    );
    await user.click(await screen.findByRole("menuitem", { name: /^cue$/i }));
    await user.click(await screen.findByRole("button", { name: /run: cue/i }));

    expect(await screen.findByRole("dialog")).toBeInTheDocument();
    expect(await screen.findByLabelText(/album artist/i)).toHaveValue("Test Artist");
    expect(screen.getByLabelText(/album title/i)).toHaveValue("Local Album");
    expect(screen.getByDisplayValue("Track One")).toBeInTheDocument();
    expect(
      screen.getByRole("checkbox", {
        name: /delete source after successful split/i,
      }),
    ).toBeChecked();
    expect(screen.getByRole("button", { name: /check/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /split/i })).toBeInTheDocument();
  });

  it("validates CUE editor and disables split when required fields are missing", async () => {
    const user = userEvent.setup();
    renderPage();
    await user.click(await screen.findByRole("button", { name: /Local Album/i }));
    await user.click(
      await screen.findByRole("button", { name: /choose album action/i }),
    );
    await user.click(await screen.findByRole("menuitem", { name: /^cue$/i }));
    await user.click(await screen.findByRole("button", { name: /run: cue/i }));
    const title = await screen.findByLabelText(/album title/i);
    await user.clear(title);
    await user.click(screen.getByRole("button", { name: /check/i }));

    expect(await screen.findByText(/album title is required/i)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /split/i })).toBeDisabled();
  });

  it("lets CUE FILE be edited before split", async () => {
    let splitAudioPath: string | null = null;
    server.use(
      http.get("/api/v1/library/albums/:id/cue", () =>
        HttpResponse.json({
          cue_files: [{ path: "Test Artist/Local Album/album.cue", selected: true }],
          document: {
            cue_path: "Test Artist/Local Album/album.cue",
            audio_path: "album.wv",
            audio_format: "wv",
            album_title: "Local Album",
            album_artist: "Test Artist",
            year: 2020,
            genre: "Pop",
            comment: "Vinyl rip",
            extra_fields: [],
            tracks: [
              {
                number: 1,
                artist: "Test Artist",
                title: "Track One",
                genre: "Pop",
                start_index: "00:00:00",
                pregap: null,
                duration: "00:01:00",
                selected: true,
              },
            ],
          },
          validation: { valid: true, issues: [] },
        }),
      ),
      http.post("/api/v1/library/albums/:id/cue/split", async ({ request }) => {
        const body = (await request.json()) as {
          document?: { audio_path?: string };
        };
        splitAudioPath = body.document?.audio_path ?? null;
        return HttpResponse.json({ job_id: 7 }, { status: 202 });
      }),
    );
    const user = userEvent.setup();
    renderPage();
    await user.click(await screen.findByRole("button", { name: /Local Album/i }));
    await user.click(
      await screen.findByRole("button", { name: /choose album action/i }),
    );
    await user.click(await screen.findByRole("menuitem", { name: /^cue$/i }));
    await user.click(await screen.findByRole("button", { name: /run: cue/i }));

    const file = await screen.findByLabelText(/^file$/i);
    await user.clear(file);
    await user.type(file, "album.flac");
    await user.click(screen.getByRole("button", { name: /check/i }));
    await user.click(await screen.findByRole("button", { name: /split/i }));

    await waitFor(() => expect(splitAudioPath).toBe("album.flac"));
  });

  it("shows failed CUE split under the first track instead of the album header", async () => {
    server.use(
      http.get("/api/v1/library/albums/:id/cue/latest", () =>
        HttpResponse.json({
          job: {
            id: 7,
            album_id: 1,
            status: "failed",
            tracks_total: 7,
            tracks_done: 0,
            progress_pct: 0,
            error_message: "CUE split input must be FLAC",
            created_at: "2026-01-01T00:00:00Z",
            updated_at: "2026-01-01T00:00:00Z",
          },
        }),
      ),
    );

    const user = userEvent.setup();
    renderPage();
    await user.click(await screen.findByRole("button", { name: /Local Album/i }));

    expect(await screen.findByText(/split failed/i)).toBeInTheDocument();
    expect(screen.getByText(/CUE split input must be FLAC/i)).toBeInTheDocument();
    expect(screen.queryByText(/^CUE:/i)).not.toBeInTheDocument();
  });
});
