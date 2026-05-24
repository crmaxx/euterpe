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
    {
      album_api_id: "inlibalbum",
      qobuz_id: 888,
      title: "In Lib Album",
      artist_name: "Test Artist",
      in_library: true,
      local_album_id: 1,
      local_cover_path: "Test Artist/Local Album/cover.jpg",
      cover_url: "https://example.com/cover.jpg",
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
      torrent_incoming_dir: "/data/torrent-incoming",
      ui: { theme: "system", locale: "en", default_quality: 6 },
    }),
  ),

  http.get("/api/v1/settings/ui", () =>
    HttpResponse.json({
      settings: { theme: "system", locale: "en", default_quality: 6 },
    }),
  ),

  http.patch("/api/v1/settings/ui", async ({ request }) => {
    const body = (await request.json()) as Record<string, unknown>;
    return HttpResponse.json({
      settings: {
        theme: (body.theme as string) ?? "system",
        locale: (body.locale as string) ?? "en",
        default_quality: (body.default_quality as number) ?? 6,
      },
    });
  }),

  http.get("/api/v1/settings/converter", () =>
    HttpResponse.json({
      settings: {
        auto_enabled: false,
        file_policy: "sibling_then_delete",
        parallelism: 5,
        formats: ["wav", "m4a", "ape"],
        flac_encode: { preset: "balanced", block_size: null, multithread: false },
      },
    }),
  ),

  http.patch("/api/v1/settings/converter", async ({ request }) => {
    const body = (await request.json()) as Record<string, unknown>;
    return HttpResponse.json({
      settings: {
        auto_enabled: (body.auto_enabled as boolean) ?? false,
        file_policy:
          (body.file_policy as string) ?? "sibling_then_delete",
        parallelism: (body.parallelism as number) ?? 5,
        formats: (body.formats as string[]) ?? ["wav", "m4a", "ape"],
        flac_encode: (body.flac_encode as object) ?? {
          preset: "balanced",
          block_size: null,
          multithread: false,
        },
      },
    });
  }),

  http.get("/api/v1/settings/library-scan", () =>
    HttpResponse.json({
      settings: {
        worker_total: 10,
        enum_workers: 5,
        process_workers: 5,
        seed_depth: 0,
        index_queue_capacity: 256,
        path_queue_capacity: 256,
      },
    }),
  ),

  http.patch("/api/v1/settings/library-scan", async ({ request }) => {
    const body = (await request.json()) as Record<string, unknown>;
    return HttpResponse.json({
      settings: {
        worker_total: (body.worker_total as number) ?? 10,
        enum_workers: (body.enum_workers as number) ?? 5,
        process_workers: (body.process_workers as number) ?? 5,
        seed_depth: (body.seed_depth as number) ?? 0,
        index_queue_capacity: 256,
        path_queue_capacity: 256,
      },
    });
  }),

  http.get("/api/v1/settings/downloads", () =>
    HttpResponse.json({ settings: { concurrency: 3 } }),
  ),

  http.patch("/api/v1/settings/downloads", async ({ request }) => {
    const body = (await request.json()) as Record<string, unknown>;
    return HttpResponse.json({
      settings: { concurrency: (body.concurrency as number) ?? 3 },
    });
  }),

  http.get("/api/v1/settings/torrent", () =>
    HttpResponse.json({
      disable_upload: true,
      max_upload_kib_per_sec: 0,
    }),
  ),

  http.patch("/api/v1/settings/torrent", async ({ request }) => {
    const body = (await request.json()) as Record<string, unknown>;
    return HttpResponse.json({
      disable_upload: (body.disable_upload as boolean) ?? true,
      max_upload_kib_per_sec: (body.max_upload_kib_per_sec as number) ?? 0,
    });
  }),

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
          source: "qobuz",
          display_title: "Artist — Album",
          qobuz_id: 99,
          quality: 6,
          progress_pct: 10,
          download_speed_bps: 512000,
          queue_position: 1,
          created_at: "2026-01-01",
          updated_at: "2026-01-01",
        },
        {
          id: 2,
          status: "completed",
          job_type: "album",
          source: "qobuz",
          display_title: "Other — Done",
          qobuz_id: 100,
          quality: 6,
          progress_pct: 100,
          download_speed_bps: 0,
          queue_position: 2,
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
        files_processed: 10,
        files_indexed: 10,
        files_total: 10,
        started_at: "2026-01-01T00:00:00Z",
        finished_at: "2026-01-01T00:01:00Z",
      },
    }),
  ),

  http.post("/api/v1/library/scan", ({ request }) => {
    const url = new URL(request.url);
    const root = url.searchParams.get("root");
    if (root?.includes("..")) {
      return HttpResponse.json(
        { error: { code: "BAD_REQUEST", message: "root must not contain '..'" } },
        { status: 400 },
      );
    }
    return HttpResponse.json({ scan_id: root ? 2 : 1 }, { status: 202 });
  }),

  http.delete("/api/v1/library/scan/:id", () => new HttpResponse(null, { status: 204 })),

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
          has_cue_files: true,
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

  http.get("/api/v1/library/albums/:id/convert/latest", () =>
    new HttpResponse(null, { status: 404 }),
  ),

  http.post("/api/v1/library/albums/:id/convert", () =>
    HttpResponse.json({ job_id: 1 }, { status: 202 }),
  ),

  http.get("/api/v1/library/albums/:id", ({ params }) =>
    HttpResponse.json({
      id: Number(params.id),
      title: "Local Album",
      artist_name: "Test Artist",
      has_convertible_tracks: false,
      has_cue_files: true,
      year: 2020,
      cover_path: null,
      genre: "Pop",
      track_total: 10,
      disc_total: 1,
      tracks: [
        {
          id: 1,
          title: "Track One",
          track_number: 1,
          year: 2020,
          disc_number: 1,
          genre: "Pop",
          path: "Test Artist/Local Album/t1.flac",
        },
      ],
    }),
  ),

  http.patch("/api/v1/library/albums/:id", async ({ request, params }) => {
    const body = (await request.json()) as Record<string, unknown>;
    return HttpResponse.json({
      id: Number(params.id),
      title: (body.album_title as string) ?? "Local Album",
      artist_name: (body.artist_name as string) ?? "Test Artist",
      has_convertible_tracks: false,
      has_cue_files: true,
      year: (body.year as number) ?? 2020,
      cover_path: null,
      genre: (body.genre as string) ?? "Pop",
      track_total: (body.track_total as number) ?? 10,
      disc_total: (body.disc_total as number) ?? 1,
      tracks: [
        {
          id: 1,
          title: "Track One",
          track_number: 1,
          year: 2020,
          disc_number: 1,
          genre: "Pop",
          path: "Test Artist/Local Album/t1.flac",
        },
      ],
    });
  }),

  http.get("/api/v1/library/albums/:id/cue", () =>
    HttpResponse.json({
      cue_files: [{ path: "Test Artist/Local Album/album.cue", selected: true }],
      document: {
        cue_path: "Test Artist/Local Album/album.cue",
        audio_path: "album.flac",
        audio_format: "flac",
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

  http.post("/api/v1/library/albums/:id/cue/validate", async ({ request }) => {
    const body = (await request.json()) as {
      document?: { album_title?: string; year?: number | null; genre?: string | null; tracks?: { title?: string; number?: number }[] };
    };
    const issues = [];
    if (!body.document?.album_title?.trim()) {
      issues.push({
        code: "missing_album_title",
        message: "Album title is required",
        severity: "error",
        field: "album_title",
      });
    }
    if (body.document?.year == null) {
      issues.push({
        code: "missing_album_year",
        message: "Album year is required",
        severity: "error",
        field: "year",
      });
    }
    if (!body.document?.genre?.trim()) {
      issues.push({
        code: "missing_album_genre",
        message: "Album genre is required",
        severity: "error",
        field: "genre",
      });
    }
    for (const track of body.document?.tracks ?? []) {
      if (!track.title?.trim()) {
        issues.push({
          code: "missing_track_title",
          message: "Track title is required",
          severity: "error",
          field: "tracks.title",
          track_number: track.number ?? null,
        });
      }
    }
    return HttpResponse.json({ valid: issues.length === 0, issues });
  }),

  http.post("/api/v1/library/albums/:id/cue/split", () =>
    HttpResponse.json({ job_id: 7 }, { status: 202 }),
  ),

  http.get("/api/v1/library/albums/:id/cue/latest", () =>
    HttpResponse.json({ job: null }),
  ),

  http.get("/api/v1/library/tracks/:id/stream", () =>
    HttpResponse.arrayBuffer(new ArrayBuffer(128), {
      headers: {
        "Content-Type": "audio/wav",
        "Accept-Ranges": "bytes",
      },
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
