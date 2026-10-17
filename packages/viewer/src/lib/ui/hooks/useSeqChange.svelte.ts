import { untrack } from "svelte";

/**
 * Run `fn` whenever the monotonic counter `seq` changes. A counter, not the
 * state it accompanies, is the trigger, so reaching that state by other means
 * (e.g. clicking a highlight changes the active id but not the counter) does
 * nothing. `fn` runs untracked: only `seq` re-runs the effect.
 *
 * By default `fn` also runs on the effect's first run, so a counter bumped
 * before this mounted is still acted on. `skipInitial` takes the value at
 * creation as the baseline instead, for one-shot requests (like focusing a
 * reply box) that must not replay on mount.
 */
export function useSeqChange(seq: () => number, fn: () => void, skipInitial = false): void {
  let last = skipInitial ? untrack(seq) : null;
  $effect(() => {
    const current = seq();
    if (current === last) return;
    last = current;
    untrack(fn);
  });
}
