import { describe, it, expect, vi, afterEach } from "vitest";
import { flushSync } from "svelte";
import { useScrollIntoViewOnNav } from "./useScrollIntoViewOnNav.svelte";

let teardown: (() => void) | null = null;

afterEach(() => {
  teardown?.();
  teardown = null;
});

describe("useScrollIntoViewOnNav", () => {
  it("scrolls the target into view (centered) each time the counter changes", () => {
    const target = { scrollIntoView: vi.fn() };
    let bump!: () => void;

    teardown = $effect.root(() => {
      let seq = $state(0);
      bump = () => (seq += 1);
      useScrollIntoViewOnNav(
        () => seq,
        () => target as unknown as Element,
      );
    });
    flushSync(); // initial run: 0 !== -1 → scrolls once

    expect(target.scrollIntoView).toHaveBeenCalledTimes(1);
    expect(target.scrollIntoView).toHaveBeenLastCalledWith({ behavior: "auto", block: "center" });

    bump();
    flushSync();
    expect(target.scrollIntoView).toHaveBeenCalledTimes(2);
  });

  it("does not scroll when an unrelated reactive value changes but the counter does not", () => {
    const target = { scrollIntoView: vi.fn() };
    let setActive!: (b: boolean) => void;

    teardown = $effect.root(() => {
      const seq = $state(0);
      let active = $state(false);
      setActive = (b) => (active = b);
      useScrollIntoViewOnNav(
        () => seq,
        () => (active ? (target as unknown as Element) : null),
      );
    });
    flushSync(); // initial: seq 0 !== -1 → findTarget reads active(false) → null → no scroll

    expect(target.scrollIntoView).not.toHaveBeenCalled();

    // activeId-style change without a counter bump must not scroll.
    setActive(true);
    flushSync();
    expect(target.scrollIntoView).not.toHaveBeenCalled();
  });

  it("is a no-op when findTarget returns null", () => {
    let bump!: () => void;

    teardown = $effect.root(() => {
      let seq = $state(0);
      bump = () => (seq += 1);
      useScrollIntoViewOnNav(
        () => seq,
        () => null,
      );
    });
    flushSync();
    bump();
    expect(() => flushSync()).not.toThrow();
  });
});
