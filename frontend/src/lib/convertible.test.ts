import { describe, expect, it } from "vitest";
import { isConvertiblePath } from "./convertible";

describe("isConvertiblePath", () => {
  it("recognizes WavPack extensions", () => {
    expect(isConvertiblePath("Scorpions/Lonesome Crow/track.wv")).toBe(true);
    expect(isConvertiblePath("Scorpions/Lonesome Crow/track.wavpack")).toBe(true);
  });
});
