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
