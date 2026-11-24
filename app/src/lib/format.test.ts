import { describe, expect, it } from "vitest";
import { formatRatio } from "./format";

describe("formatRatio", () => {
  it("formats the upload/download ratio for a session", () => {
    expect(formatRatio(500, 1_000)).toBe("0.50×");
    expect(formatRatio(2_500, 1_000)).toBe("2.50×");
    expect(formatRatio(0, 1_000)).toBe("0.00×");
  });

  it("leaves the ratio unavailable until there is a download denominator", () => {
    expect(formatRatio(0, 0)).toBe("—");
    expect(formatRatio(null, 10)).toBe("—");
    expect(formatRatio(Number.NaN, 10)).toBe("—");
  });
});
