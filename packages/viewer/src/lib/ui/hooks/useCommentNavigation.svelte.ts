export interface CommentNavigationDeps {
  /** Current navigable comment ids, in order. Read fresh on each keypress. */
  navigable: () => string[];
  /** Move the active comment one step; returns the new position to announce,
   *  or null when there is nothing to navigate. */
  navigate: (direction: "next" | "prev") => { index: number; total: number; author: string } | null;
}

export interface CommentNavigation {
  /** Live-region text describing the current position, updated on each jump. */
  readonly announcement: string;
}

/** True when the target is a field that swallows typed characters, so bare
 *  `n`/`p` should be treated as text input, not navigation. */
function isEditable(el: EventTarget | null): boolean {
  if (!(el instanceof HTMLElement)) return false;
  const tag = el.tagName;
  return tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT" || el.isContentEditable;
}

/** Global keyboard navigation between page comments. Mount once (in Layout).
 *  `n` -> next comment, `p` -> previous; from idle, `n` jumps to the first and
 *  `p` to the last. Dependencies are passed in (rather than read from context)
 *  so this stays a domain-agnostic kit hook, like useActiveHeading. This is the
 *  viewer's only global key handler — every other handler is scoped to a
 *  specific overlay/form. */
export function useCommentNavigation(deps: CommentNavigationDeps): CommentNavigation {
  let announcement = $state("");
  // A polite live region only re-announces when its text node *changes*, but
  // navigate() wraps around: stepping on a single-comment page (or onto a thread
  // by the same author at the same index) returns the same position, so the
  // human-readable string is byte-identical and the screen reader stays silent.
  // Toggle an invisible marker on the end of each announcement so the text node
  // always differs and the move is re-announced. We use U+200B (zero-width
  // space): unlike a trailing normal or non-breaking space — which screen
  // readers' text normalization strips as trailing whitespace (both have the
  // Unicode White_Space property), leaving an identical announced string and
  // defeating the re-announce — U+200B is not White_Space, so it survives, yet
  // NVDA/JAWS/VoiceOver don't pronounce it at default verbosity. A per-instance
  // toggle (no shared module state) keeps the hook self-contained.
  let marked = false;

  $effect(() => {
    function onKeydown(e: KeyboardEvent) {
      // Let browser/OS shortcuts (Cmd+N, Ctrl+P, …) through untouched.
      if (e.metaKey || e.ctrlKey || e.altKey) return;
      if (e.key !== "n" && e.key !== "p") return;
      // Don't hijack typing in the comment form (or any host input when
      // embedded). Check both the event target and the focused element: they
      // usually match, but can differ in embedded/cross-frame setups where the
      // keydown bubbles to a target outside the element that actually has focus.
      if (isEditable(e.target) || isEditable(document.activeElement)) return;
      if (deps.navigable().length === 0) return;

      e.preventDefault();
      const result = deps.navigate(e.key === "n" ? "next" : "prev");
      if (result) {
        // Omit "by <author>" when the author name is blank, so the live region
        // never announces a dangling "Comment 1 of 2 by ".
        const by = result.author ? ` by ${result.author}` : "";
        // Read the marker, then flip it: the first announcement is unmarked, so
        // an isolated press still produces the plain "Comment N of M by X".
        const mark = marked ? "\u200B" : "";
        marked = !marked;
        announcement = `Comment ${result.index + 1} of ${result.total}${by}${mark}`;
      }
    }
    window.addEventListener("keydown", onKeydown);
    return () => window.removeEventListener("keydown", onKeydown);
  });

  return {
    get announcement() {
      return announcement;
    },
  };
}
