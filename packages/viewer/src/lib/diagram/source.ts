/** What a rendered diagram is: an inline SVG or a PNG. */
const SOURCE_SELECTOR = "svg, img";

/**
 * The first direct-child element of `root` matching `selector`.
 *
 * Deliberately not `root.querySelector(":scope > " + selector)`: jsdom's
 * `:scope` combinator support is broken for `ShadowRoot` (it works fine on a
 * plain `Element`, but returns null on a shadow root even when the child is
 * right there — confirmed against jsdom 29 / nwsapi 2.3.9). Walking `children`
 * with `Element.matches()` is the same "direct children only" semantics
 * without going through `:scope` at all, so it works identically on both a
 * `Element` and a `ShadowRoot` in every environment.
 */
function directChild<T extends Element>(root: ParentNode, selector: string): T | null {
  for (const child of root.children) {
    if (child.matches(selector)) return child as T;
  }
  return null;
}

/**
 * The rendered diagram inside a figure: an inline `<svg>` or a PNG `<img>`.
 *
 * A server-rendered figure holds its SVG inside a `<rw-diagram>` shadow root;
 * a hand-authored `<figure class="diagram">` in markdown holds it as a direct
 * child. Both are supported — the raw form keeps working exactly as before, it
 * simply does not get id isolation.
 *
 * Scoped to direct children on purpose: the injected `.diagram-expand-btn`
 * lives in the same figure and carries its own icon `<svg>`, which an unscoped
 * lookup would return for a figure whose diagram failed to render.
 */
export function diagramSource(figure: Element): SVGSVGElement | HTMLImageElement | null {
  const wrapper = directChild<HTMLElement>(figure, "rw-diagram");
  const root: ParentNode = wrapper?.shadowRoot ?? figure;
  return directChild<SVGSVGElement | HTMLImageElement>(root, SOURCE_SELECTOR);
}

/**
 * Every diagram shadow root under `container`.
 *
 * `querySelectorAll` does not pierce shadow roots, so any traversal that must
 * reach inside diagrams (rewriting section-ref links on diagram anchors, say)
 * needs these roots explicitly alongside the container.
 */
export function diagramShadowRoots(container: ParentNode): ShadowRoot[] {
  const roots: ShadowRoot[] = [];
  for (const el of container.querySelectorAll("rw-diagram")) {
    if (el.shadowRoot) roots.push(el.shadowRoot);
  }
  return roots;
}
