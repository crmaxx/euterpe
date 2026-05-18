import { describe, expect, it } from "vitest";
import { formatQualityLabel } from "./quality";

describe("formatQualityLabel", () => {
  it("maps known Qobuz format ids", () => {
    expect(formatQualityLabel(5)).toBe("MP3 320");
    expect(formatQualityLabel(6)).toBe("FLAC CD");
    expect(formatQualityLabel(7)).toBe("FLAC Hi-Res");
    expect(formatQualityLabel(27)).toBe("FLAC Hi-Res+");
  });

  it("falls back for unknown ids", () => {
    expect(formatQualityLabel(99)).toBe("quality 99");
  });
});
