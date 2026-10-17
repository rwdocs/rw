import { describe, it, expect, vi, afterEach } from "vitest";
import { flushSync } from "svelte";
import { useSeqChange } from "./useSeqChange.svelte";

let teardown: (() => void) | null = null;

afterEach(() => {
  teardown?.();
  teardown = null;
});

function mount(skipInitial?: boolean) {
  const fn = vi.fn();
  let bump!: () => void;
  let setOther!: (n: number) => void;
  teardown = $effect.root(() => {
    let seq = $state(3);
    let other = $state(0);
    bump = () => (seq += 1);
    setOther = (n) => (other = n);
    useSeqChange(
      () => seq,
      () => {
        void other;
        fn();
      },
      skipInitial,
    );
  });
  flushSync();
  return { fn, bump, setOther };
}

describe("useSeqChange", () => {
  it("runs on the first run and on every bump by default", () => {
    const { fn, bump } = mount();
    expect(fn).toHaveBeenCalledTimes(1);

    bump();
    flushSync();
    expect(fn).toHaveBeenCalledTimes(2);
  });

  it("with skipInitial, ignores the value at creation and runs on the next bump", () => {
    const { fn, bump } = mount(true);
    expect(fn).not.toHaveBeenCalled();

    bump();
    flushSync();
    expect(fn).toHaveBeenCalledTimes(1);
  });

  it("does not re-run when state read by fn changes without a bump", () => {
    const { fn, setOther } = mount();
    expect(fn).toHaveBeenCalledTimes(1);

    setOther(1);
    flushSync();
    expect(fn).toHaveBeenCalledTimes(1);
  });
});
