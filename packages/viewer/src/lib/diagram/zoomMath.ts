/** A width/height pair in CSS pixels. */
export interface Size {
  w: number;
  h: number;
}

/**
 * An SVG `viewBox` — the rectangle of the diagram's user-unit coordinate space
 * mapped onto the (fixed-size) SVG element. Zoom/pan is expressed by moving and
 * resizing this rectangle, which the SVG rasterizer redraws crisply at every
 * scale (a repaint, not a layout reflow, and never a bitmap stretch — so it
 * stays sharp and smooth, unlike a CSS `transform: scale()`).
 */
export interface ViewBox {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** Upper bound on zoom, as a multiple of the diagram's natural size. */
export const MAX_SCALE = 8;

/** Clamp `scale` into `[min, max]`. */
export function clampScale(scale: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, scale));
}

/**
 * Parse an SVG `viewBox` attribute string into a {@link ViewBox}, or `null` when
 * it is absent, malformed, or has a non-positive width/height. The single parser
 * shared by the modal (as a full `ViewBox`) and natural-size detection (which
 * drops x/y).
 */
export function parseViewBox(attr: string | null): ViewBox | null {
  if (!attr) return null;
  const p = attr
    .trim()
    .split(/[\s,]+/)
    .map(Number);
  if (p.length === 4 && p.every(Number.isFinite) && p[2] > 0 && p[3] > 0) {
    return { x: p[0], y: p[1], w: p[2], h: p[3] };
  }
  return null;
}

/**
 * The zoom at which the whole diagram fits inside `viewport` after reserving
 * `margin` px on every edge, as a multiple of natural size. May be > 1 when the
 * diagram is smaller than the usable area — the initial view fills the popup
 * (SVG upscales crisply), so a small diagram is enlarged rather than left tiny.
 */
export function fitScale(diagram: ViewBox, natural: Size, viewport: Size, margin: number): number {
  const usableW = Math.max(1, viewport.w - margin * 2);
  const usableH = Math.max(1, viewport.h - margin * 2);
  // px-per-user-unit that fits the diagram's extent, then converted back to a
  // scale (multiple of natural) via the uniform base ratio.
  const pxPerUnitFit = Math.min(usableW / diagram.w, usableH / diagram.h);
  const basePxPerUnit = natural.w / diagram.w;
  return pxPerUnitFit / basePxPerUnit;
}

/**
 * The viewBox width/height that renders the diagram at zoom `scale` (1 = natural
 * pixel size). Larger scale → smaller viewBox (a smaller slice of the diagram
 * fills the fixed SVG box, i.e. magnified).
 *
 * Both axes use the SAME px-per-user-unit (derived from the width), so the
 * viewBox always has the viewport's aspect ratio. That is what keeps the render
 * undistorted: the SVG fills the viewport with `preserveAspectRatio` set to
 * `meet`, and because the viewBox already matches the viewport aspect there is
 * no letterboxing — the mapping from viewport px to user units stays linear (so
 * the pan/zoom math below is exact). Kroki's `preserveAspectRatio="none"` SVGs
 * would otherwise stretch to the popup's aspect ratio.
 */
export function viewBoxSizeForScale(
  diagram: ViewBox,
  natural: Size,
  viewport: Size,
  scale: number,
): Size {
  const pxPerUnit = (natural.w / diagram.w) * scale;
  return { w: viewport.w / pxPerUnit, h: viewport.h / pxPerUnit };
}

/** The zoom implied by a viewBox width (inverse of {@link viewBoxSizeForScale}). */
export function scaleOfViewBox(
  diagram: ViewBox,
  natural: Size,
  viewport: Size,
  viewBoxW: number,
): number {
  return (viewport.w * diagram.w) / (natural.w * viewBoxW);
}

/**
 * The opening viewBox: the diagram at natural size (100%), or scaled down to fit
 * when it is larger than the viewport (`min(1, fitScale)`), centered. A small
 * diagram opens at 100% rather than being blown up to fill the popup.
 */
export function initialViewBox(
  diagram: ViewBox,
  natural: Size,
  viewport: Size,
  margin: number,
): ViewBox {
  const scale = Math.min(1, fitScale(diagram, natural, viewport, margin));
  const { w, h } = viewBoxSizeForScale(diagram, natural, viewport, scale);
  return {
    x: diagram.x + diagram.w / 2 - w / 2,
    y: diagram.y + diagram.h / 2 - h / 2,
    w,
    h,
  };
}

/**
 * Zoom to `targetScale` (clamped to `[minScale, maxScale]`) while keeping the
 * diagram point currently under the viewport pixel (px, py) fixed.
 */
export function zoomViewBox(
  vb: ViewBox,
  diagram: ViewBox,
  natural: Size,
  viewport: Size,
  px: number,
  py: number,
  targetScale: number,
  minScale: number,
  maxScale: number,
): ViewBox {
  const scale = clampScale(targetScale, minScale, maxScale);
  const { w, h } = viewBoxSizeForScale(diagram, natural, viewport, scale);
  // Diagram-space point currently under the pointer.
  const ux = vb.x + (px / viewport.w) * vb.w;
  const uy = vb.y + (py / viewport.h) * vb.h;
  // Re-anchor the new viewBox so that point stays under the pointer.
  return { x: ux - (px / viewport.w) * w, y: uy - (py / viewport.h) * h, w, h };
}

/** Pan by a screen-pixel delta (drag), translating it into user units. */
export function panViewBox(vb: ViewBox, viewport: Size, dxPx: number, dyPx: number): ViewBox {
  return {
    x: vb.x - (dxPx / viewport.w) * vb.w,
    y: vb.y - (dyPx / viewport.h) * vb.h,
    w: vb.w,
    h: vb.h,
  };
}
