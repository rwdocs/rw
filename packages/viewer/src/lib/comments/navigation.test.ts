// packages/viewer/src/lib/comments/navigation.test.ts
import { describe, it, expect } from "vitest";
import { resolveNavTarget, sortByOrder } from "./navigation";

describe("resolveNavTarget", () => {
  const list = ["a", "b", "c"];

  it("returns null for an empty list", () => {
    expect(resolveNavTarget([], null, "next")).toBeNull();
    expect(resolveNavTarget([], "a", "prev")).toBeNull();
  });

  it("enters at the first comment from idle on next", () => {
    expect(resolveNavTarget(list, null, "next")).toBe("a");
  });

  it("enters at the last comment from idle on prev", () => {
    expect(resolveNavTarget(list, null, "prev")).toBe("c");
  });

  it("treats an unknown active id as idle", () => {
    expect(resolveNavTarget(list, "zzz", "next")).toBe("a");
    expect(resolveNavTarget(list, "zzz", "prev")).toBe("c");
  });

  it("steps forward and backward in the middle", () => {
    expect(resolveNavTarget(list, "a", "next")).toBe("b");
    expect(resolveNavTarget(list, "b", "prev")).toBe("a");
  });

  it("wraps from the last to the first on next", () => {
    expect(resolveNavTarget(list, "c", "next")).toBe("a");
  });

  it("wraps from the first to the last on prev", () => {
    expect(resolveNavTarget(list, "a", "prev")).toBe("c");
  });

  it("stays put with a single-item list", () => {
    expect(resolveNavTarget(["only"], "only", "next")).toBe("only");
    expect(resolveNavTarget(["only"], "only", "prev")).toBe("only");
  });
});

describe("sortByOrder", () => {
  it("orders items by their rank in the order array", () => {
    const items = [{ id: "b" }, { id: "a" }, { id: "c" }];
    expect(sortByOrder(items, ["a", "b", "c"]).map((i) => i.id)).toEqual(["a", "b", "c"]);
  });

  it("places unranked items last in their original relative order", () => {
    const items = [{ id: "x" }, { id: "a" }, { id: "y" }];
    expect(sortByOrder(items, ["a"]).map((i) => i.id)).toEqual(["a", "x", "y"]);
  });

  it("does not mutate the input array", () => {
    const items = [{ id: "b" }, { id: "a" }];
    sortByOrder(items, ["a", "b"]);
    expect(items.map((i) => i.id)).toEqual(["b", "a"]);
  });
});
