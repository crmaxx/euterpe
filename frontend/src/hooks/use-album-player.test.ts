import { describe, expect, it } from "vitest";
import { howlerFormatFromPath } from "./use-album-player";

describe("howlerFormatFromPath", () => {
  it("detects flac from library path", () => {
    expect(
      howlerFormatFromPath(
        "Absu/1999 - The Sun Of Tiphareth/01 - Apzu.flac",
      ),
    ).toEqual(["flac"]);
  });

  it("detects mp3", () => {
    expect(howlerFormatFromPath("Artist/Album/track.mp3")).toEqual(["mp3"]);
  });
});
