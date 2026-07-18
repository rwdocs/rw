/**
 * CSS for the inside of a diagram shadow root.
 *
 * A shadow root is a separate style scope, so the light-DOM
 * `.prose figure.diagram svg` rules in `content.css` cannot reach a wrapped
 * diagram. These declarations are the way in. The two rule sets are mutually
 * exclusive by construction — `.prose figure.diagram svg` can only ever match
 * an *un-wrapped* (hand-authored) figure — so they never both apply.
 *
 * Nothing here may depend on the theme: `.dark` sits on the document root, and
 * no selector inside a shadow root can match an ancestor class
 * (`:host-context()` is Chromium-only). Theme-dependent rules go on a
 * light-DOM host instead — see the invert rules in `content.css`.
 *
 * Keep in sync with the `.prose figure.diagram svg text` and
 * `.prose figure.diagram svg a` rules in `src/styles/content.css`, which serve
 * un-wrapped figures. Those rules are not adjacent to each other in that file;
 * each carries a pointer back here.
 */
export const DIAGRAM_SHARED_CSS = `
svg text { font-family: "Roboto", sans-serif !important; }
svg a { text-decoration: none !important; }
`.trim();

/**
 * Article sizing: Rust pins the SVG's physical size, `max-width` keeps a large
 * diagram inside the column, and `height: auto` preserves the aspect ratio.
 *
 * `display: block` is load-bearing and has no counterpart in `content.css`. An
 * un-wrapped SVG is a direct child of the `display: flex` figure, so it is
 * blockified automatically. Inside the wrapper it is an ordinary inline-level
 * element, which builds a line box and adds ~9px of descender space below every
 * diagram. Blockifying restores exact parity with the un-wrapped shape.
 *
 * Keep in sync with the `.prose figure.diagram svg` sizing rule in
 * `src/styles/content.css`, which gives un-wrapped figures the same treatment.
 */
export const DIAGRAM_ARTICLE_CSS = `
${DIAGRAM_SHARED_CSS}
svg { display: block; max-width: 100%; height: auto !important; }
`.trim();

/**
 * Popup sizing: the SVG fills the viewport box and its viewBox drives zoom, so
 * it must be block-level and stretch — the opposite of the article's intrinsic
 * sizing.
 */
export const DIAGRAM_MODAL_CSS = `
${DIAGRAM_SHARED_CSS}
svg { display: block; width: 100%; height: 100%; }
`.trim();

/** One constructed sheet per distinct CSS string — see `sheetFor`. */
const sheetCache = new Map<string, CSSStyleSheet>();

/**
 * The shared `CSSStyleSheet` for `css`, constructing it on first use.
 *
 * Memoized by CSS string so every article root adopts one object and every
 * modal root adopts another, rather than one sheet per diagram. Returns `null`
 * where constructable stylesheets are unavailable (jsdom, older browsers) or
 * where parsing throws, so the caller can fall back to a `<style>` element.
 */
export function sheetFor(css: string): CSSStyleSheet | null {
  const cached = sheetCache.get(css);
  if (cached) return cached;
  if (typeof CSSStyleSheet !== "function") return null;
  try {
    const sheet = new CSSStyleSheet();
    sheet.replaceSync(css);
    sheetCache.set(css, sheet);
    return sheet;
  } catch {
    return null;
  }
}

/**
 * Attach `css` to a shadow root, preferring a constructable stylesheet.
 *
 * `adoptedStyleSheets` shares one sheet object across every diagram root that
 * takes the same CSS (see `sheetFor`), which costs less memory than a `<style>`
 * per root. It is unavailable in jsdom (and in older browsers), so fall back to
 * an inline `<style>` — equivalent in effect, and the extra cost is per-root
 * memory rather than render time.
 *
 * Only the fallback path is reachable under jsdom. The adopted path is asserted
 * by the Playwright diagram-isolation spec ("a wrapped diagram is styled by the
 * sheet its shadow root adopts"), which runs in a real browser: it checks the
 * root took this branch (one adopted sheet, no `<style>` fallback) and that the
 * declarations land — an over-wide SVG clamped to its column, and `svg text`
 * resolving to Roboto.
 */
export function applySheet(root: ShadowRoot, css: string): void {
  if ("adoptedStyleSheets" in root) {
    const sheet = sheetFor(css);
    // Defensive, not a live hazard: every caller applies a sheet exactly once
    // per root (`RwDiagram.connectedCallback` returns early once a shadow root
    // exists, and `ensureCloneRoot` short-circuits on an unchanged host). Should
    // a future caller re-style a root, adopting the shared sheet twice would be
    // harmless to rendering but would grow the array without bound.
    if (sheet && !root.adoptedStyleSheets.includes(sheet)) {
      root.adoptedStyleSheets = [...root.adoptedStyleSheets, sheet];
      return;
    }
    if (sheet) return;
  }
  const style = document.createElement("style");
  style.textContent = css;
  root.appendChild(style);
}
