import { describe, expect, it } from "vitest";
import { http, HttpResponse } from "msw";
import { api, type LibraryAlbumSort, type SortOrder } from "./client";
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

  it("sends download status filter", async () => {
    let status: string | null = null;
    server.use(
      http.get("/api/v1/downloads", ({ request }) => {
        status = new URL(request.url).searchParams.get("status");
        return HttpResponse.json({ items: [], next_cursor: null, has_more: false });
      }),
    );

    await api.downloads({ status: "failed" });

    expect(status).toBe("failed");
  });

  it.each([
    ["album_date", "desc"],
    ["date_added", "asc"],
  ] satisfies [LibraryAlbumSort, SortOrder][])(
    "sends %s library album sort",
    async (expectedSort, expectedOrder) => {
    let sort: string | null = null;
    let order: string | null = null;
    server.use(
      http.get("/api/v1/library/albums", ({ request }) => {
        const params = new URL(request.url).searchParams;
        sort = params.get("sort");
        order = params.get("order");
        return HttpResponse.json({ items: [], next_cursor: null, has_more: false });
      }),
    );

      await api.libraryAlbums({ sort: expectedSort, order: expectedOrder });

      expect(sort).toBe(expectedSort);
      expect(order).toBe(expectedOrder);
    },
  );

  it("purges completed downloads", async () => {
    let method: string | null = null;
    server.use(
      http.post("/api/v1/downloads/purge", ({ request }) => {
        method = request.method;
        return HttpResponse.json({ deleted: 2 });
      }),
    );

    const response = await api.purgeCompletedDownloads();

    expect(method).toBe("POST");
    expect(response.deleted).toBe(2);
  });

  it("retries failed downloads in bulk", async () => {
    let method: string | null = null;
    server.use(
      http.post("/api/v1/downloads/retry", ({ request }) => {
        method = request.method;
        return HttpResponse.json({ retried: 3 });
      }),
    );

    const response = await api.retryFailedDownloads();

    expect(method).toBe("POST");
    expect(response.retried).toBe(3);
  });

  it("throws ApiClientError on 401", async () => {
    await expect(
      api.testLogin({ user_id: 1, auth_token: "bad" }),
    ).rejects.toMatchObject({
      code: "QOBUZ_AUTH_FAILED",
    } satisfies Partial<ApiClientError>);
  });
});
