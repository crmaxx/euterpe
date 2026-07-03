import { describe, expect, it } from "vitest";
import { http, HttpResponse } from "msw";
import { api } from "./client";
import { ApiClientError } from "./errors";
import { server } from "@/test/msw/server";

describe("api client", () => {
  it("fetches favorites", async () => {
    const data = await api.favorites({ library_filter: "all" });
    expect(data.items.length).toBeGreaterThanOrEqual(1);
    expect(data.items[0].album_api_id).toBe("zg7pv28g4mldg");
  });

  it("sends explicit all favorites filter", async () => {
    let filter: string | null = null;
    server.use(
      http.get("/api/v1/qobuz/favorites", ({ request }) => {
        filter = new URL(request.url).searchParams.get("library_filter");
        return HttpResponse.json({ items: [], next_cursor: null, has_more: false });
      }),
    );

    await api.favorites({ library_filter: "all" });

    expect(filter).toBe("all");
  });

  it("sends explicit not-in-library favorites filter", async () => {
    let filter: string | null = null;
    server.use(
      http.get("/api/v1/qobuz/favorites", ({ request }) => {
        filter = new URL(request.url).searchParams.get("library_filter");
        return HttpResponse.json({ items: [], next_cursor: null, has_more: false });
      }),
    );

    await api.favorites({ library_filter: "not_in_library" });

    expect(filter).toBe("not_in_library");
  });

  it("throws ApiClientError on 401", async () => {
    await expect(
      api.testLogin({ user_id: 1, auth_token: "bad" }),
    ).rejects.toMatchObject({
      code: "QOBUZ_AUTH_FAILED",
    } satisfies Partial<ApiClientError>);
  });
});
