import { parseViewBox, type Size } from "./zoomMath";

/** Parse a length attribute as px; returns null for missing, non-numeric, or non-px (e.g. "100%"). */
function parsePx(value: string | null): number | null {
  if (!value) return null;
  const m = /^\s*([0-9]*\.?[0-9]+)(px)?\s*$/.exec(value);
  if (!m) return null;
  const n = Number(m[1]);
  return Number.isFinite(n) && n > 0 ? n : null;
}

/** Read the [w, h] of an SVG's viewBox, or null if absent/malformed. */
function viewBoxSize(svg: SVGSVGElement): Size | null {
  const vb = parseViewBox(svg.getAttribute("viewBox"));
  return vb ? { w: vb.w, h: vb.h } : null;
}

/**
 * The diagram's true natural pixel size, treated as "100%" in the zoom popup.
 *
 * For an SVG: px width/height attributes if present, else the viewBox, else the
 * live bounding box, else 1x1. For an IMG: intrinsic size, else width/height
 * attributes, else the bounding box.
 */
export function naturalSizeOf(el: SVGSVGElement | HTMLImageElement): Size {
  if (el instanceof HTMLImageElement) {
    if (el.naturalWidth > 0 && el.naturalHeight > 0) {
      return { w: el.naturalWidth, h: el.naturalHeight };
    }
    const w = parsePx(el.getAttribute("width"));
    const h = parsePx(el.getAttribute("height"));
    if (w && h) return { w, h };
    const rect = el.getBoundingClientRect();
    return { w: Math.max(1, rect.width), h: Math.max(1, rect.height) };
  }

  const w = parsePx(el.getAttribute("width"));
  const h = parsePx(el.getAttribute("height"));
  if (w && h) return { w, h };

  const vb = viewBoxSize(el);
  if (vb) return vb;

  const rect = el.getBoundingClientRect();
  if (rect.width > 0 && rect.height > 0) return { w: rect.width, h: rect.height };

  return { w: 1, h: 1 };
}
