import { describe, expect, it } from "vitest";
import { api } from "./client";
import { ApiClientError } from "./errors";

describe("api client", () => {
  it("fetches favorites", async () => {
    const data = await api.favorites();
    expect(data.items).toHaveLength(1);
    expect(data.items[0].album_api_id).toBe("zg7pv28g4mldg");
  });

  it("throws ApiClientError on 401", async () => {
    await expect(
      api.testLogin({ user_id: 1, auth_token: "bad", persist: false }),
    ).rejects.toMatchObject({
      code: "QOBUZ_AUTH_FAILED",
    } satisfies Partial<ApiClientError>);
  });
});
