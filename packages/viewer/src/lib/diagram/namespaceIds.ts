// Monotonic counter so successive clones (and any briefly-coexisting detached
// one) never share a prefix. A module-level counter is enough — only one modal
// clone is live at a time.
let seq = 0;

// `url(#id)` inside a presentation attribute, inline style, or <style> block.
// Handles optional matching quotes: url(#a), url('#a'), url("#a").
const URL_REF = /\burl\(\s*(['"]?)#([^)'"]+)\1\s*\)/g;

// A `#ident` CSS id-selector token: letter/underscore/non-ASCII start (so
// `#узел1` renames like any other id), then ident chars. Matches greedily, so
// `#containerfoo` is rewritten only if "containerfoo" itself is a mapped id —
// never a partial match on "container". Hex colors never match (digit
// leading), but an id spelling a hex-safe word (e.g. "cafe") can still collide
// with one; this has no CSS parser behind it. CSS-escaped idents aren't
// handled (real diagram output never emits them); non-ident id shapes inside
// `url(#…)` are URL_REF's job.
const CSS_ID_TOKEN = /#([A-Za-z_\u0080-\uffff][\w\u0080-\uffff-]*)/g;

// Attributes whose value is a space-separated list of id references.
const ARIA_IDREF_ATTRS = new Set(["aria-labelledby", "aria-describedby"]);

// Splitter for those idref lists that keeps the whitespace runs, so the
// rewritten list preserves the original separators.
const IDREF_SPLIT = /(\s+)/;

/**
 * Rewrite every `id` in `root` — and every reference to it — to a unique,
 * collision-free name, so a cloned SVG becomes fully self-contained.
 *
 * A deep clone of an inline diagram SVG duplicates all of its `id`s while the
 * original still lives in the article. SVG paint references
 * (`marker-end="url(#arrow)"`, `fill="url(#grad)"`, `clip-path`, a gradient's
 * `href="#base"`, `<use href="#sym">`) resolve **document-wide to the first
 * matching id**, so the clone would silently borrow the original's `<defs>`.
 * That renders fine only while the original is present — it breaks when the
 * article is replaced (a same-page live reload) and is mishandled outright by
 * some WebKit versions. Namespacing the clone removes the shared coupling.
 *
 * Rewrites: `id` attributes; `url(#id)` in any attribute and inline `style`;
 * `#id` tokens (selectors and `url(#id)` fragments) in `<style>` CSS;
 * `href` / `xlink:href` fragment references (`#id`); and space-separated
 * `aria-labelledby` / `aria-describedby` idref lists.
 * References to ids not defined inside `root` (external/unknown) are left alone.
 */
export function namespaceIds(root: SVGElement): void {
  const idEls = [root, ...Array.from(root.querySelectorAll<SVGElement>("[id]"))].filter((el) =>
    el.hasAttribute("id"),
  );
  if (idEls.length === 0) return;

  const prefix = `dzm${seq++}-`;
  const map = new Map<string, string>();
  for (const el of idEls) {
    const oldId = el.getAttribute("id")!;
    if (map.has(oldId)) continue;
    const newId = prefix + oldId;
    map.set(oldId, newId);
    el.setAttribute("id", newId);
  }

  const rewriteUrls = (value: string): string =>
    value.replace(URL_REF, (whole, _q, id) => {
      const mapped = map.get(id);
      return mapped ? `url(#${mapped})` : whole;
    });

  for (const el of [root, ...Array.from(root.querySelectorAll<SVGElement>("*"))]) {
    for (const attr of Array.from(el.attributes)) {
      let value = attr.value;
      if (value.includes("url(")) value = rewriteUrls(value);
      // href / xlink:href both report localName "href"; a leading "#" marks a
      // local fragment reference. Setting attr.value updates in place, keeping
      // the attribute's namespace intact.
      if (attr.localName === "href" && value.startsWith("#")) {
        const mapped = map.get(value.slice(1));
        if (mapped) value = `#${mapped}`;
      }
      // aria-labelledby / aria-describedby hold space-separated id lists
      // (Mermaid's accTitle/accDescr point the root at <title>/<desc> ids).
      if (ARIA_IDREF_ATTRS.has(attr.localName)) {
        value = value
          .split(IDREF_SPLIT)
          .map((token) => map.get(token) ?? token)
          .join("");
      }
      if (value !== attr.value) attr.value = value;
    }
  }

  // Mermaid scopes every CSS rule under the SVG root id (`#container .node
  // {…}`), so <style> selectors must be renamed in lockstep with the id
  // attributes or the clone loses all styling. Run URL_REF before
  // CSS_ID_TOKEN: the map keys are the ORIGINAL ids, so CSS_ID_TOKEN
  // re-scanning an already-rewritten `url(#dzm0-grad)` fragment is a safe
  // no-op, not a double-rewrite.
  const rewriteCssIdTokens = (css: string): string =>
    css.replace(CSS_ID_TOKEN, (whole, id) => {
      const mapped = map.get(id);
      return mapped ? `#${mapped}` : whole;
    });

  for (const style of root.querySelectorAll("style")) {
    const css = style.textContent;
    if (!css) continue;
    const rewritten = rewriteCssIdTokens(rewriteUrls(css));
    if (rewritten !== css) style.textContent = rewritten;
  }
}
