export interface CommentNavigationDeps {
  /** Current navigable comment ids, in order. Read fresh on each keypress. */
  navigable: () => string[];
  /** Move the active comment one step; returns the new position to announce,
   *  or null when there is nothing to navigate. */
  navigate: (direction: "next" | "prev") => { index: number; total: number; author: string } | null;
  /** Focus the active thread's reply box; returns the active thread's position
   *  to announce, or null when there is nothing to reply to. */
  requestReplyFocus: () => { index: number; total: number; author: string } | null;
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

/** Resolve a keydown to its mnemonic letter, independent of keyboard layout.
 *  Prefer the produced character when it is a Latin letter — so Dvorak/AZERTY
 *  and user remaps win — and fall back to the physical key position for
 *  non-Latin layouts (Cyrillic, Greek, …), where `key` is not a Latin letter
 *  but `code` still reports the QWERTY slot the user pressed. Returns null when
 *  the press maps to no single letter. */
function shortcutKey(e: KeyboardEvent): string | null {
  if (/^[a-z]$/i.test(e.key)) return e.key.toLowerCase();
  const m = /^Key([A-Z])$/.exec(e.code);
  return m ? m[1].toLowerCase() : null;
}

/** " by <author>" suffix for a live-region announcement, omitted when the author
 *  name is blank so the live region never reads a dangling "… by ". */
function authorSuffix(author: string): string {
  return author ? ` by ${author}` : "";
}

/** Global keyboard navigation between page comments. Mount once (in Layout).
 *  `n` -> next comment, `p` -> previous; from idle, `n` jumps to the first and
 *  `p` to the last. `r` focuses the active thread's reply box. Dependencies are
 *  passed in (rather than read from context) so this stays a domain-agnostic
 *  kit hook, like useActiveHeading. This is the viewer's only global key
 *  handler — every other handler is scoped to a specific overlay/form. */
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

  // Set the live region, toggling the invisible marker so the text node always
  // differs from the previous one and the move is re-announced even when the
  // human-readable string repeats. The first announcement is unmarked, so an
  // isolated press still produces the plain text.
  function announce(text: string) {
    const mark = marked ? "​" : "";
    marked = !marked;
    announcement = `${text}${mark}`;
  }

  $effect(() => {
    function onKeydown(e: KeyboardEvent) {
      // Let browser/OS shortcuts (Cmd+N, Ctrl+P, …) through untouched. AltGr is
      // checked separately: on Linux/ChromeOS it sets neither altKey nor
      // ctrlKey, so without this an AltGr glyph on physical KeyN/KeyP/KeyR would
      // reach the code fallback below and fire a spurious shortcut.
      if (e.metaKey || e.ctrlKey || e.altKey || e.getModifierState("AltGraph")) return;
      const key = shortcutKey(e);
      if (key !== "n" && key !== "p" && key !== "r") return;
      // Don't hijack typing in the comment form (or any host input when
      // embedded). Check both the event target and the focused element: they
      // usually match, but can differ in embedded/cross-frame setups where the
      // keydown bubbles to a target outside the element that actually has focus.
      if (isEditable(e.target) || isEditable(document.activeElement)) return;

      if (key === "r") {
        // A null result (nothing to reply to) passes through untouched — no
        // preventDefault — so the keypress isn't swallowed when there's no
        // thread to act on.
        const result = deps.requestReplyFocus();
        if (!result) return;
        e.preventDefault();
        announce(
          `Replying to comment ${result.index + 1} of ${result.total}${authorSuffix(result.author)}`,
        );
        return;
      }

      if (deps.navigable().length === 0) return;

      e.preventDefault();
      const result = deps.navigate(key === "n" ? "next" : "prev");
      if (result) {
        announce(`Comment ${result.index + 1} of ${result.total}${authorSuffix(result.author)}`);
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
