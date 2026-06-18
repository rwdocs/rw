// packages/viewer/src/lib/comments/navigation.ts

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
