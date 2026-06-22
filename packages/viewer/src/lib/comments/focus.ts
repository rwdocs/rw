/**
 * Release the comment composer after a submit/dismiss so `n`/`p` navigation
 * (which ignores keys while an editable field is focused) resumes.
 *
 * Moves focus to the thread element the user just acted on when one is given —
 * keeping keyboard/screen-reader users oriented. When the element can't be
 * resolved (a race with a live-refresh, a hidden sidebar), it blurs the active
 * editable element instead, so focus is never left trapped in the textarea.
 */
export function restoreFocusToThread(el: HTMLElement | null | undefined): void {
  if (el) {
    el.focus({ preventScroll: true });
    return;
  }
  // No thread element to land on (a race, a hidden sidebar): release the
  // composer if it still holds focus so n/p navigation resumes. Only blur the
  // textarea itself — never yank focus from wherever it may have legitimately
  // moved during the async gap before this runs.
  const active = document.activeElement;
  if (active instanceof HTMLTextAreaElement) active.blur();
}

/**
 * Move keyboard focus into a thread's reply textarea (the `r` shortcut),
 * scrolling it into view first so the user can see what they type on a long
 * thread.
 *
 * Deferred to `requestAnimationFrame` so a sidebar card that is
 * `visibility:hidden` until its anchor is measured has flipped to visible —
 * `focus()` on a `visibility:hidden` element is a spec no-op (the same reason
 * `CommentForm`'s autofocus defers). Bails when the textarea is in a
 * `display:none` subtree (the comments aside is hidden at narrow widths), where
 * `offsetParent` is null and both scroll and focus would silently do nothing.
 */
export function focusReplyTextarea(textarea: HTMLTextAreaElement | null | undefined): void {
  if (!textarea) return;
  requestAnimationFrame(() => {
    if (textarea.offsetParent === null) return;
    textarea.scrollIntoView({ block: "nearest" });
    textarea.focus({ preventScroll: true });
  });
}
