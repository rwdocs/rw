/**
 * Scroll a target element into view (centered) whenever a monotonic navigation
 * counter changes. Used to bring the active comment into view on keyboard
 * navigation.
 *
 * The counter — not the active id — is the trigger, so activating a comment by
 * other means (e.g. clicking its highlight, which changes the active id but not
 * the counter) never yanks the page. The guard compares against the last value
 * handled by this instance; because the counter is strictly monotonic it can
 * never collide with a stale value.
 *
 * Domain-agnostic: the caller injects the counter and a thunk that locates the
 * element to scroll (returning null/undefined when the current target isn't
 * this caller's to handle), so the hook reads no shared state and stays in the
 * kit layer — like `useActiveHeading`.
 */
export function useScrollIntoViewOnNav(
  seq: () => number,
  findTarget: () => Element | null | undefined,
): void {
  let lastSeq = -1;
  $effect(() => {
    const current = seq();
    if (current === lastSeq) return;
    lastSeq = current;
    findTarget()?.scrollIntoView({ behavior: "auto", block: "center" });
  });
}
