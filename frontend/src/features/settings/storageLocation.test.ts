import { describe, expect, it } from "vitest";
import {
  childBrowsePath,
  formatSmbLocation,
  joinBrowsePath,
  parseSmbLocation,
  storageLabel,
} from "@/features/settings/storageLocation";

describe("storageLocation", () => {
  it("formats and parses smb urls with path", () => {
    const url = formatSmbLocation({
      host: "192.168.0.124",
      share: "dietpi",
      path: "Musik",
    });
    expect(url).toBe("smb://192.168.0.124/dietpi/Musik");
    expect(parseSmbLocation(url)).toEqual({
      host: "192.168.0.124",
      port: 445,
      share: "dietpi",
      path: "Musik",
    });
  });

  it("builds storage label for smb", () => {
    expect(
      storageLabel({
        kind: "smb",
        host: "nas",
        port: 445,
        share: "music",
        path: "lib",
        watch_status: { state: "disabled" },
        password_configured: true,
      }),
    ).toBe("smb://nas/music/lib");
  });

  it("joins browse paths relative to library root", () => {
    expect(joinBrowsePath("Musik/Flac", "Aarni")).toBe("Musik/Flac/Aarni");
    expect(childBrowsePath("Aarni", "Album")).toBe("Aarni/Album");
    expect(joinBrowsePath("Musik/Flac", "")).toBe("Musik/Flac");
  });
});
