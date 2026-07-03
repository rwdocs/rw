// Monotonic counter so successive clones (and any briefly-coexisting detached
// one) never share a prefix. A module-level counter is enough — only one modal
// clone is live at a time.
let seq = 0;

// `url(#id)` inside a presentation attribute, inline style, or <style> block.
// Handles optional matching quotes: url(#a), url('#a'), url("#a").
const URL_REF = /\burl\(\s*(['"]?)#([^)'"]+)\1\s*\)/g;

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
 * Rewrites: `id` attributes; `url(#id)` in any attribute, inline `style`, and
 * `<style>` blocks; and `href` / `xlink:href` fragment references (`#id`).
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
      if (value !== attr.value) attr.value = value;
    }
  }

  // url(#id) can also live in the CSS text of embedded <style> blocks (Mermaid).
  for (const style of root.querySelectorAll("style")) {
    const css = style.textContent;
    if (css && css.includes("url(")) style.textContent = rewriteUrls(css);
  }
}
