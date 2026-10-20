const PARAM = "rw-files";

/**
 * Point each same-origin image under `root` at a URL that carries `version`;
 * version 0 leaves the images as rendered.
 *
 * A browser shows an image it has already loaded in this document again
 * without revalidating it, even under `Cache-Control: no-cache` and in a new
 * `<img>`, so a file changed on disk shows only under a new URL.
 */
export function refreshImages(root: ParentNode, version: number) {
  if (version === 0) return;
  for (const img of root.querySelectorAll("img")) {
    const src = img.getAttribute("src")?.trim();
    // These name the page itself.
    if (!src || src.startsWith("#") || src.startsWith("?")) continue;
    const url = new URL(src, document.baseURI);
    if (url.origin !== location.origin) continue;
    url.searchParams.set(PARAM, String(version));
    const versioned = url.pathname + url.search + url.hash;
    if (versioned !== src) {
      img.setAttribute("src", versioned);
    }
  }
}
