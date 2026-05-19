import { beforeAll, describe, expect, it, vi } from "vitest";
import {
  hasImageMagic,
  isActiveAlbumCoverBlobUrl,
  setAlbumCoverBlobUrl,
} from "./albumCoverBlobCache";

const MIN_JPEG = new Uint8Array([
  0xff, 0xd8, 0xff, 0xe0, 0x00, 0x10, 0x4a, 0x46, 0x49, 0x46, 0x00, 0x01,
]);

beforeAll(() => {
  vi.stubGlobal("URL", {
    ...URL,
    revokeObjectURL: vi.fn(),
  });
});

describe("hasImageMagic", () => {
  it("detects JPEG without image/* Content-Type", () => {
    expect(hasImageMagic(MIN_JPEG)).toBe(true);
  });

  it("rejects empty bytes", () => {
    expect(hasImageMagic(new Uint8Array())).toBe(false);
  });
});

describe("isActiveAlbumCoverBlobUrl", () => {
  it("rejects blob URLs not present in sync cache", () => {
    expect(isActiveAlbumCoverBlobUrl(7, "blob:stale")).toBe(false);
    setAlbumCoverBlobUrl("7", "blob:current");
    expect(isActiveAlbumCoverBlobUrl(7, "blob:current")).toBe(true);
    expect(isActiveAlbumCoverBlobUrl(7, "blob:stale")).toBe(false);
  });
});
