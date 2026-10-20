import { onViewerKeydown } from "../viewerKeydown";

export interface CommentPosition {
  index: number;
  total: number;
  author: string;
}

export interface CommentNavigationDeps {
  /** An element inside the viewer: under `mountRw()` keys from the host page
   *  are left alone (see `onViewerKeydown`). */
  element: () => Element | undefined;
  /** Move the active comment one step; returns the new position to announce,
   *  or null when there is nothing to navigate. */
  navigate: (direction: "next" | "prev") => CommentPosition | null;
  /** Focus the active thread's reply box; returns the active thread's position
   *  to announce, or null when there is nothing to reply to. */
  requestReplyFocus: () => CommentPosition | null;
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

/** Page-wide keyboard navigation between page comments. Mount once (in Layout).
 *  `n` -> next comment, `p` -> previous; from idle, `n` jumps to the first and
 *  `p` to the last. `r` focuses the active thread's reply box. Dependencies are
 *  passed in (rather than read from context) so this stays a domain-agnostic
 *  kit hook, like useActiveHeading. */
export function useCommentNavigation(deps: CommentNavigationDeps): CommentNavigation {
  let announcement = $state("");
  // A polite live region re-announces only when its text changes, and
  // navigate() can repeat a position (a single-comment page wraps onto itself).
  // Toggling a trailing U+200B keeps the text changing: screen readers strip
  // trailing White_Space (non-breaking space included) but keep U+200B, which
  // NVDA/JAWS/VoiceOver don't pronounce at default verbosity.
  let marked = false;

  // The first announcement is unmarked, so an isolated press reads plain text.
  function announce(text: string) {
    const mark = marked ? "​" : "";
    marked = !marked;
    announcement = `${text}${mark}`;
  }

  $effect(() => {
    const element = deps.element();
    if (!element) return;

    function onKeydown(e: KeyboardEvent) {
      // Let browser/OS shortcuts (Cmd+N, Ctrl+P, …) through untouched. AltGr is
      // checked separately: on Linux/ChromeOS it sets neither altKey nor
      // ctrlKey, so without this an AltGr glyph on physical KeyN/KeyP/KeyR would
      // reach the code fallback below and fire a spurious shortcut.
      if (e.metaKey || e.ctrlKey || e.altKey || e.getModifierState("AltGraph")) return;
      const key = shortcutKey(e);
      if (key !== "n" && key !== "p" && key !== "r") return;
      // Don't hijack typing in the comment form. Check both the event target
      // and the focused element: they usually match, but can differ in
      // embedded/cross-frame setups where the keydown bubbles to a target
      // outside the element that actually has focus.
      if (isEditable(e.target) || isEditable(document.activeElement)) return;

      const result =
        key === "r" ? deps.requestReplyFocus() : deps.navigate(key === "n" ? "next" : "prev");
      // Nothing to act on: leave the keypress unconsumed.
      if (!result) return;
      e.preventDefault();
      const prefix = key === "r" ? "Replying to comment" : "Comment";
      announce(`${prefix} ${result.index + 1} of ${result.total}${authorSuffix(result.author)}`);
    }
    return onViewerKeydown(element, onKeydown);
  });

  return {
    get announcement() {
      return announcement;
    },
  };
}
