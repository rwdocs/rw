/** Class marking the injected expand button (also used to guard against double-injection). */
export const EXPAND_BUTTON_CLASS = "diagram-expand-btn";

// Inline expand/fullscreen icon (arrows to four corners). Uses currentColor.
const ICON_SVG = `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" focusable="false"><path d="M8 3H5a2 2 0 0 0-2 2v3m18 0V5a2 2 0 0 0-2-2h-3M3 16v3a2 2 0 0 0 2 2h3m8 0h3a2 2 0 0 0 2-2v-3"/></svg>`;

/**
 * Inject an "Expand diagram" button into every `<figure class="diagram">` in
 * `container` (skipping `.diagram-error` figures), wiring each button's click to
 * `onOpen(figure)`. Idempotent. Returns a cleanup that removes the injected
 * buttons and their listeners.
 *
 * Mirrors `initializeTabs`: call it from a `$effect` keyed on `page.data` and
 * `articleRef` only, so buttons are torn down and re-injected in lockstep with
 * the server-rendered article HTML.
 */
export function initializeDiagramZoom(
  container: HTMLElement,
  onOpen: (figure: HTMLElement) => void,
): () => void {
  const figures = container.querySelectorAll<HTMLElement>("figure.diagram:not(.diagram-error)");
  const injected: Array<{ button: HTMLButtonElement; handler: () => void }> = [];

  for (const figure of figures) {
    // Idempotency guard: never add a second button to the same figure.
    if (figure.querySelector(`.${EXPAND_BUTTON_CLASS}`)) continue;

    const button = document.createElement("button");
    button.type = "button";
    button.className = EXPAND_BUTTON_CLASS;
    button.setAttribute("aria-label", "Expand diagram");
    button.innerHTML = ICON_SVG;

    const handler = () => onOpen(figure);
    button.addEventListener("click", handler);

    figure.appendChild(button);
    injected.push({ button, handler });
  }

  return () => {
    for (const { button, handler } of injected) {
      button.removeEventListener("click", handler);
      button.remove();
    }
  };
}
