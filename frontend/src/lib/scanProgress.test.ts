import { describe, expect, it } from "vitest";
import {
  estimateScanEta,
  formatDuration,
  scanProgressPercent,
} from "./scanProgress";

describe("scanProgressPercent", () => {
  it("returns undefined while total not known", () => {
    expect(scanProgressPercent(0, 0)).toBeUndefined();
    expect(scanProgressPercent(3, 0)).toBeUndefined();
  });

  it("computes indexed vs total", () => {
    expect(scanProgressPercent(5, 10)).toBe(50);
    expect(scanProgressPercent(10, 10)).toBe(100);
  });
});

describe("formatDuration", () => {
  it("formats seconds and minutes", () => {
    expect(formatDuration(45)).toBe("45s");
    expect(formatDuration(125)).toBe("2m 5s");
  });
});

describe("estimateScanEta", () => {
  it("estimates from indexed rate when total is known", () => {
    const t0 = 1_000_000;
    const eta = estimateScanEta(10, 20, [
      { t: t0, filesIndexed: 5, filesTotal: 20 },
      { t: t0 + 10_000, filesIndexed: 10, filesTotal: 20 },
    ]);
    expect(eta).toMatch(/^~/);
  });

  it("returns null during discover phase", () => {
    expect(estimateScanEta(0, 0, [])).toBeNull();
  });
});
