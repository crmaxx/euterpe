import "@testing-library/jest-dom/vitest";
import { afterAll, afterEach, beforeAll, vi } from "vitest";
import { clearAlbumCoverBlobCache } from "@/features/library/albumCoverBlobCache";
import { server } from "./msw/server";

/** jsdom logs "Not implemented: navigation" when code sets `location.href`. */
let locationHref = "http://localhost/";

const locationMock = {
  origin: "http://localhost",
  protocol: "http:",
  host: "localhost",
  hostname: "localhost",
  port: "",
  pathname: "/",
  search: "",
  hash: "",
  assign: vi.fn((url: string | URL) => {
    locationHref = String(url);
  }),
  replace: vi.fn(),
  reload: vi.fn(),
  toString: () => locationHref,
};

Object.defineProperty(locationMock, "href", {
  configurable: true,
  get: () => locationHref,
  set: (url: string) => {
    locationHref = url;
  },
});

Object.defineProperty(window, "location", {
  configurable: true,
  value: locationMock,
});

const storage = new Map<string, string>();
vi.stubGlobal("localStorage", {
  getItem: (key: string) => storage.get(key) ?? null,
  setItem: (key: string, value: string) => {
    storage.set(key, value);
  },
  removeItem: (key: string) => {
    storage.delete(key);
  },
  clear: () => storage.clear(),
});

beforeAll(() => server.listen({ onUnhandledRequest: "error" }));
afterEach(() => {
  server.resetHandlers();
  clearAlbumCoverBlobCache();
  storage.clear();
  locationHref = "http://localhost/";
  vi.mocked(locationMock.assign).mockClear();
  vi.mocked(locationMock.replace).mockClear();
  vi.mocked(locationMock.reload).mockClear();
});
afterAll(() => server.close());
