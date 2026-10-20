// @vitest-environment node

import { describe, it, expect } from "vitest";
import { encodePath } from "./encodePath";

describe("encodePath", () => {
  it.each([
    ["", ""],
    ["/", "/"],
    ["/guide", "/guide"],
    ["a?b", "a%3Fb"],
    ["c#", "c%23"],
    ["100%", "100%25"],
    ["my page", "my%20page"],
    ["/гайд", "/%D0%B3%D0%B0%D0%B9%D0%B4"],
    ["/billing/c#/intro", "/billing/c%23/intro"],
    ["a+b&c=d:e", "a%2Bb%26c%3Dd%3Ae"],
  ])("encodes %j as %j", (path, expected) => {
    expect(encodePath(path)).toBe(expected);
  });
});
