import { DIAGRAM_ARTICLE_CSS, applySheet } from "./sheet";

const TAG = "rw-diagram";

/**
 * Host element that gives one rendered diagram its own id-resolution scope.
 *
 * Kroki generators emit ids unique only within a single SVG, so several inlined
 * diagrams on one page collide: `url(#clip1)` resolves document-wide to the
 * first match, and a diagram silently renders with another's clip paths. A
 * shadow root is a separate tree scope for id and style resolution, which makes
 * the collision structurally impossible.
 *
 * The server emits this wrapper (`rw-kroki`'s `svg_figure`). Custom elements
 * upgrade synchronously through `innerHTML` (it is a `[CEReactions]` setter), so
 * Svelte's `{@html}` cannot show a frame of unisolated content. Declarative
 * Shadow DOM would be simpler but does not work via `innerHTML` at all.
 */
class RwDiagram extends HTMLElement {
  connectedCallback(): void {
    // Re-connecting (a live reload moving nodes) must not attach twice —
    // attachShadow throws on a second call, and the children have already moved.
    if (this.shadowRoot) return;
    const root = this.attachShadow({ mode: "open" });
    applySheet(root, DIAGRAM_ARTICLE_CSS);
    root.append(...this.childNodes);
  }
}

/**
 * Define `rw-diagram`, once.
 *
 * The guard is not decoration: `define()` throws on re-registration, which is
 * reachable in Backstage where `mountRw` may run inside a host app that bundles
 * the viewer twice, and under HMR.
 */
export function registerRwDiagram(): void {
  if (typeof customElements === "undefined") return;
  if (customElements.get(TAG)) return;
  customElements.define(TAG, RwDiagram);
}

registerRwDiagram();
