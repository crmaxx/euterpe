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
  total: 1,
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
      ],
    }),
  ),

  http.post("/api/v1/downloads", () =>
    HttpResponse.json({ job_id: 42 }, { status: 202 }),
  ),

  http.delete("/api/v1/qobuz/favorites", () => new HttpResponse(null, { status: 204 })),

  http.delete("/api/v1/downloads/:id", () => new HttpResponse(null, { status: 204 })),
];
