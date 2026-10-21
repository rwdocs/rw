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
    ["a+b&c=d:e@f$g,h;[i]^j|k", "a+b&c=d:e@f$g,h;[i]^j|k"],
    ['a\\b"<>`{}\t\x7f', "a%5Cb%22%3C%3E%60%7B%7D%09%7F"],
  ])("encodes %j as %j", (path, expected) => {
    expect(encodePath(path)).toBe(expected);
  });
});
