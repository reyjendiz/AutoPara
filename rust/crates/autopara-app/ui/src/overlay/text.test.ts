import { describe, expect, it } from "vitest";
import { elapsed, tail } from "./text";

describe("overlay text", () => {
  it("formats elapsed time", () => {
    expect(elapsed(0)).toBe("0:00");
    expect(elapsed(65_400)).toBe("1:05");
    expect(elapsed(3_723_000)).toBe("1:02:03");
  });

  it("keeps only the newest lines", () => {
    expect(tail([1, 2, 3], 4, 3)).toEqual([2, 3, 4]);
    expect(tail([], "a", 3)).toEqual(["a"]);
  });
});
