import { describe, expect, it } from "vitest";
import { findTrackConvertProgress } from "./parseConvertFiles";

describe("findTrackConvertProgress", () => {
  it("hides successful convert rows", () => {
    expect(
      findTrackConvertProgress("Album/source.wv", [
        { path: "Album/source.wv", status: "success", progress_pct: 100 },
      ]),
    ).toBeNull();
  });

  it("keeps failed convert rows with the error", () => {
    expect(
      findTrackConvertProgress("Album/source.wv", [
        { path: "Album/source.wv", status: "failed", error: "boom" },
      ]),
    ).toEqual({ status: "failed", progressPct: undefined, error: "boom" });
  });
});
