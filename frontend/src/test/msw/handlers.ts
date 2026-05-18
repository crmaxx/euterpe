import { http, HttpResponse } from "msw";

export const mockFavorites = {
  items: [
    {
      album_api_id: "zg7pv28g4mldg",
      qobuz_id: 393908828,
      title: "Test Album",
      artist_name: "Test Artist",
      in_library: false,
    },
  ],
  next_cursor: null,
  has_more: false,
};

export const handlers = [
  http.get("/api/v1/server/info", () =>
    HttpResponse.json({
      version: "0.1.0",
      library_path: "/music",
      credentials_configured: true,
      admin_auth_required: false,
    }),
  ),

  http.get("/api/v1/qobuz/sync/latest", () =>
    HttpResponse.json({ run: null }),
  ),

  http.get("/api/v1/qobuz/favorites", () => HttpResponse.json(mockFavorites)),

  http.post("/api/v1/qobuz/sync", () =>
    HttpResponse.json({
      run_id: 1,
      albums_total: 10,
      added: 1,
      removed: 0,
    }),
  ),

  http.get("/api/v1/qobuz/connection", () =>
    HttpResponse.json({
      connected: false,
      master_key_configured: true,
    }),
  ),

  http.post("/api/v1/qobuz/logout", () => new HttpResponse(null, { status: 204 })),

  http.get("/api/v1/qobuz/oauth/start", () =>
    HttpResponse.json({
      authorize_url: "https://www.qobuz.com/signin/oauth?ext_app_id=1&redirect_url=http://localhost/callback",
      state: "test-state",
    }),
  ),

  http.post("/api/v1/qobuz/test-login", async ({ request }) => {
    const body = (await request.json()) as { auth_token?: string };
    if (body.auth_token === "bad") {
      return HttpResponse.json(
        { error: { code: "QOBUZ_AUTH_FAILED", message: "invalid token" } },
        { status: 401 },
      );
    }
    return HttpResponse.json({
      membership: "Studio",
      user_auth_token_refreshed: false,
    });
  }),

  http.get("/api/v1/downloads", () =>
    HttpResponse.json({
      items: [
        {
          id: 1,
          status: "running",
          job_type: "album",
          qobuz_id: 99,
          quality: 6,
          progress_pct: 10,
          created_at: "2026-01-01",
          updated_at: "2026-01-01",
        },
        {
          id: 2,
          status: "completed",
          job_type: "album",
          qobuz_id: 100,
          quality: 6,
          progress_pct: 100,
          created_at: "2026-01-01",
          updated_at: "2026-01-01",
        },
      ],
      next_cursor: null,
      has_more: false,
    }),
  ),

  http.post("/api/v1/downloads", () =>
    HttpResponse.json({ job_id: 42 }, { status: 202 }),
  ),

  http.delete("/api/v1/qobuz/favorites", () => new HttpResponse(null, { status: 204 })),

  http.post("/api/v1/downloads/by-url", async ({ request }) => {
    const body = (await request.json()) as { url?: string };
    if (!body.url?.trim()) {
      return HttpResponse.json(
        { error: { code: "BAD_REQUEST", message: "url must not be empty" } },
        { status: 400 },
      );
    }
    return HttpResponse.json({ job_id: 99 }, { status: 202 });
  }),

  http.post("/api/v1/downloads/purge", () =>
    HttpResponse.json({ deleted: 1 }),
  ),

  http.delete("/api/v1/downloads/:id", ({ request }) => {
    const url = new URL(request.url);
    if (url.searchParams.get("purge") === "1") {
      return new HttpResponse(null, { status: 204 });
    }
    return new HttpResponse(null, { status: 204 });
  }),

  http.get("/api/v1/library/scan/latest", () =>
    HttpResponse.json({
      run: {
        id: 1,
        status: "success",
        files_seen: 10,
        files_indexed: 10,
        started_at: "2026-01-01T00:00:00Z",
        finished_at: "2026-01-01T00:01:00Z",
      },
    }),
  ),

  http.post("/api/v1/library/scan", () =>
    HttpResponse.json({ scan_id: 1 }, { status: 202 }),
  ),

  http.get("/api/v1/library/albums", () =>
    HttpResponse.json({
      items: [
        {
          id: 1,
          title: "Local Album",
          artist_name: "Test Artist",
          year: 2020,
          track_count: 2,
          cover_path: null,
        },
      ],
      next_cursor: null,
      has_more: false,
    }),
  ),

  http.get("/api/v1/library/albums/:id/cover", () =>
    new HttpResponse(null, { status: 404 }),
  ),

  http.put("/api/v1/library/albums/:id/cover", () =>
    HttpResponse.json({ cover_path: "album/cover.jpg", tracks_embedded: 2 }),
  ),

  http.get("/api/v1/library/albums/:id", ({ params }) =>
    HttpResponse.json({
      id: Number(params.id),
      title: "Local Album",
      artist_name: "Test Artist",
      year: 2020,
      cover_path: null,
      tracks: [
        {
          id: 1,
          title: "Track One",
          track_number: 1,
          year: 2020,
          disc_number: 1,
          genre: "Pop",
          path: "a/t1.flac",
        },
      ],
    }),
  ),

  http.get("/api/v1/library/tracks/:id", ({ params }) =>
    HttpResponse.json({
      id: Number(params.id),
      album_id: 1,
      title: "Track One",
      artist_name: "Test Artist",
      album_title: "Local Album",
      track_number: 1,
      year: 2020,
      disc_number: 1,
      genre: "Pop",
      path: "a/t1.flac",
    }),
  ),

  http.get("/api/v1/integrations/catalog", () =>
    HttpResponse.json({
      items: [
        {
          provider: "musicbrainz",
          integration_type: "tag_source",
          label: "MusicBrainz",
          description: "MusicBrainz",
          requires_master_key: false,
          config_schema: [
            {
              key: "contact",
              label: "Contact email",
              field_type: "string",
              required: true,
              secret: false,
              placeholder: "you@example.com",
            },
          ],
        },
      ],
    }),
  ),

  http.get("/api/v1/integrations", () =>
    HttpResponse.json({
      items: [
        {
          id: 1,
          integration_type: "tag_source",
          provider: "musicbrainz",
          display_name: "MusicBrainz",
          enabled: true,
          config: { contact: "test@example.com" },
          has_secrets: false,
          sort_order: 0,
          created_at: "2026-01-01",
          updated_at: "2026-01-01",
        },
      ],
    }),
  ),

  http.post("/api/v1/integrations", async ({ request }) => {
    const body = (await request.json()) as { provider?: string };
    return HttpResponse.json(
      {
        item: {
          id: 2,
          integration_type: "tag_source",
          provider: body.provider ?? "musicbrainz",
          display_name: "New",
          enabled: true,
          config: {},
          has_secrets: false,
          sort_order: 1,
          created_at: "2026-01-01",
          updated_at: "2026-01-01",
        },
      },
      { status: 201 },
    );
  }),

  http.patch("/api/v1/integrations/:id", async ({ params, request }) => {
    const body = (await request.json()) as Record<string, unknown>;
    return HttpResponse.json({
      item: {
        id: Number(params.id),
        integration_type: "tag_source",
        provider: "musicbrainz",
        display_name: "MusicBrainz",
        enabled: (body.enabled as boolean | undefined) ?? true,
        config: { contact: "test@example.com" },
        has_secrets: false,
        sort_order: 0,
        created_at: "2026-01-01",
        updated_at: "2026-01-01",
      },
    });
  }),

  http.delete("/api/v1/integrations/:id", () => new HttpResponse(null, { status: 204 })),

  http.post("/api/v1/library/albums/:id/metadata/lookup", () =>
    HttpResponse.json({
      candidates: [
        {
          id: "cand-1",
          title: "Local Album",
          artist_name: "Test Artist",
          year: 2020,
          score: 0.95,
          track_count: 1,
          source_label: "MusicBrainz",
        },
      ],
      page: 1,
      has_more: false,
    }),
  ),

  http.post("/api/v1/library/albums/:id/metadata/apply", () =>
    HttpResponse.json({
      tracks_updated: 1,
      cover_applied: false,
      warnings: [],
    }),
  ),

  http.patch("/api/v1/library/tracks/:id", async ({ params, request }) => {
    const body = (await request.json()) as Record<string, unknown>;
    return HttpResponse.json({
      id: Number(params.id),
      album_id: 1,
      title: (body.title as string | undefined) ?? "Track",
      artist_name: "Test Artist",
      album_title: "Local Album",
      track_number: (body.track_number as number | undefined) ?? 1,
      year: (body.year as number | undefined) ?? 2020,
      disc_number: (body.disc_number as number | undefined) ?? 1,
      genre: (body.genre as string | undefined) ?? "Pop",
      path: "a/t1.flac",
    });
  }),
];
