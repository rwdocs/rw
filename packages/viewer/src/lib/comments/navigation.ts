// packages/viewer/src/lib/comments/navigation.ts
import type { Comment } from "../../types/comments";

/** Resolve the comment id to navigate to.
 *
 *  `ordered` is the full navigation cycle (document order). When `activeId` is
 *  null or not in the list, navigation enters at the first item for "next" and
 *  the last for "prev" — so the same two keys cover both entry-from-idle and
 *  stepping. Steps wrap around at both ends. */
export function resolveNavTarget(
  ordered: string[],
  activeId: string | null,
  direction: "next" | "prev",
): string | null {
  if (ordered.length === 0) return null;
  const idx = activeId == null ? -1 : ordered.indexOf(activeId);
  if (idx === -1) {
    return direction === "next" ? ordered[0] : ordered[ordered.length - 1];
  }
  const delta = direction === "next" ? 1 : -1;
  const next = (idx + delta + ordered.length) % ordered.length;
  return ordered[next];
}

/** Sort items by their position in `order`. Items absent from `order` sort
 *  last, keeping their original relative order (stable). Does not mutate the
 *  input. Shared by the comment sidebar and the store's navigable list so there
 *  is a single definition of inline-thread ordering. */
export function sortByOrder<T extends { id: string }>(items: T[], order: string[]): T[] {
  const rank = new Map(order.map((id, i) => [id, i]));
  return items.toSorted((a, b) => (rank.get(a.id) ?? Infinity) - (rank.get(b.id) ?? Infinity));
}

/** True when `activeId` names a comment that is in the current orphan set but
 *  was NOT in the previous one — i.e. an open thread that just lost its anchor
 *  (a content edit / live-reload removed the passage). A comment that was
 *  already an orphan is not a transition — returns false, leaving the caller to
 *  keep it active (orphans live in the page-comments timeline and are valid n/p
 *  targets). Returns false when `activeId` is null. */
export function isNewlyOrphaned(
  activeId: string | null,
  currentOrphans: ReadonlySet<string>,
  previousOrphans: ReadonlySet<string>,
): boolean {
  return activeId != null && currentOrphans.has(activeId) && !previousOrphans.has(activeId);
}

/** Whether a thread holds its slot in the navigation cycle and on the inline
 *  surfaces (highlight + margin panel): open threads always, plus the active
 *  thread even once resolved — so resolving the thread you're sitting on doesn't
 *  make it vanish from under you mid-navigation.
 *
 *  The highlight layer and the store's `navigable` must apply this *same*
 *  predicate, not merely a similar one: `navigable`'s ordering comes from
 *  `order`, which is derived from the set of wrapped highlights, and
 *  `sortByOrder` ranks ids absent from `order` as `Infinity`. Narrowing
 *  `desiredHighlights` back to open-only would silently sort a resolved-active
 *  thread to the end of the nav cycle instead of holding its slot.
 *
 *  Does NOT govern the page-comments timeline — see `PageComments.visibleThreads`,
 *  which partitions threads into open/resolved lists and would render the active
 *  thread twice if it honoured this. */
export function holdsSlot(
  thread: { id: string; status: Comment["status"] },
  activeId: string | null,
): boolean {
  return thread.status !== "resolved" || thread.id === activeId;
}
