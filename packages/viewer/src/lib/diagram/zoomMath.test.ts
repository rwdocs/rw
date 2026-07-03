import { describe, it, expect } from "vitest";
import {
  clampScale,
  fitScale,
  viewBoxSizeForScale,
  scaleOfViewBox,
  initialViewBox,
  zoomViewBox,
  panViewBox,
  parseViewBox,
  MAX_SCALE,
  type ViewBox,
} from "./zoomMath";

// A diagram whose intrinsic viewBox (user units) matches its natural px size.
const DIAGRAM: ViewBox = { x: 0, y: 0, w: 400, h: 200 };
const NATURAL = { w: 400, h: 200 };
const VIEWPORT = { w: 1000, h: 800 };

describe("parseViewBox", () => {
  it("parses a valid viewBox string", () => {
    expect(parseViewBox("0 0 800 400")).toEqual({ x: 0, y: 0, w: 800, h: 400 });
  });

  it("tolerates commas and surrounding whitespace", () => {
    expect(parseViewBox("  10, 20, 30 , 40 ")).toEqual({ x: 10, y: 20, w: 30, h: 40 });
  });

  it("returns null for missing, malformed, or non-positive size", () => {
    expect(parseViewBox(null)).toBeNull();
    expect(parseViewBox("0 0 800")).toBeNull(); // too few
    expect(parseViewBox("0 0 800 abc")).toBeNull(); // non-numeric
    expect(parseViewBox("0 0 0 400")).toBeNull(); // zero width
    expect(parseViewBox("0 0 -5 400")).toBeNull(); // negative width
  });
});

describe("clampScale", () => {
  it("clamps below min and above max", () => {
    expect(clampScale(0.1, 0.5, 8)).toBe(0.5);
    expect(clampScale(50, 0.5, 8)).toBe(8);
    expect(clampScale(2, 0.5, 8)).toBe(2);
  });
});

describe("fitScale", () => {
  it("returns the limiting ratio that fits the diagram inside viewport minus margin", () => {
    // usable 800x600; diagram/natural 1600x400 => width-limited 0.5
    const d: ViewBox = { x: 0, y: 0, w: 1600, h: 400 };
    expect(fitScale(d, { w: 1600, h: 400 }, { w: 1000, h: 800 }, 100)).toBeCloseTo(0.5);
  });

  it("can exceed 1 for a small diagram (initial view fills the popup)", () => {
    const d: ViewBox = { x: 0, y: 0, w: 100, h: 100 };
    expect(fitScale(d, { w: 100, h: 100 }, { w: 1000, h: 1000 }, 0)).toBeCloseTo(10);
  });
});

describe("viewBoxSizeForScale / scaleOfViewBox", () => {
  it("at scale 1 the diagram renders at natural px, so viewBox = viewport scaled into user units", () => {
    // vb.w = viewport.w * diagram.w / (natural.w * 1) = 1000 * 400 / 400 = 1000
    const s = viewBoxSizeForScale(DIAGRAM, NATURAL, VIEWPORT, 1);
    expect(s.w).toBeCloseTo(1000);
    expect(s.h).toBeCloseTo(800);
  });

  it("scaleOfViewBox is the inverse", () => {
    const s = viewBoxSizeForScale(DIAGRAM, NATURAL, VIEWPORT, 2);
    expect(scaleOfViewBox(DIAGRAM, NATURAL, VIEWPORT, s.w)).toBeCloseTo(2);
  });

  it("zooming in halves the viewBox width", () => {
    const a = viewBoxSizeForScale(DIAGRAM, NATURAL, VIEWPORT, 1);
    const b = viewBoxSizeForScale(DIAGRAM, NATURAL, VIEWPORT, 2);
    expect(b.w).toBeCloseTo(a.w / 2);
  });
});

describe("initialViewBox", () => {
  it("caps the opening scale at 1 (100%) for a small diagram and centers on it", () => {
    // 400x200 fits in a 1000x800 viewport, so it opens at 100% (not blown up to
    // fill), centered on the diagram center (200,100).
    const vb = initialViewBox(DIAGRAM, NATURAL, VIEWPORT, 0);
    expect(scaleOfViewBox(DIAGRAM, NATURAL, VIEWPORT, vb.w)).toBeCloseTo(1);
    expect(vb.x + vb.w / 2).toBeCloseTo(200);
    expect(vb.y + vb.h / 2).toBeCloseTo(100);
  });

  it("fits a diagram larger than the viewport (scale < 1)", () => {
    const big: ViewBox = { x: 0, y: 0, w: 4000, h: 2000 };
    const bigNat = { w: 4000, h: 2000 };
    const vb = initialViewBox(big, bigNat, VIEWPORT, 0);
    const scale = scaleOfViewBox(big, bigNat, VIEWPORT, vb.w);
    expect(scale).toBeLessThan(1);
    // whole diagram is visible: viewBox covers the diagram's extent
    expect(vb.w).toBeGreaterThanOrEqual(big.w);
    expect(vb.h).toBeGreaterThanOrEqual(big.h);
  });
});

describe("zoomViewBox", () => {
  it("keeps the diagram point under the pointer fixed", () => {
    const vb = initialViewBox(DIAGRAM, NATURAL, VIEWPORT, 0);
    const px = 250;
    const py = 300;
    const before = { ux: vb.x + (px / VIEWPORT.w) * vb.w, uy: vb.y + (py / VIEWPORT.h) * vb.h };
    const next = zoomViewBox(vb, DIAGRAM, NATURAL, VIEWPORT, px, py, 2, 0.1, MAX_SCALE);
    const after = {
      ux: next.x + (px / VIEWPORT.w) * next.w,
      uy: next.y + (py / VIEWPORT.h) * next.h,
    };
    expect(after.ux).toBeCloseTo(before.ux);
    expect(after.uy).toBeCloseTo(before.uy);
  });

  it("clamps to maxScale", () => {
    const vb = initialViewBox(DIAGRAM, NATURAL, VIEWPORT, 0);
    const next = zoomViewBox(vb, DIAGRAM, NATURAL, VIEWPORT, 0, 0, 999, 0.1, MAX_SCALE);
    expect(scaleOfViewBox(DIAGRAM, NATURAL, VIEWPORT, next.w)).toBeCloseTo(MAX_SCALE);
  });
});

describe("panViewBox", () => {
  it("translates a screen delta into user units and moves the window opposite", () => {
    const vb: ViewBox = { x: 0, y: 0, w: 1000, h: 800 };
    // drag right by 100px on a 1000px-wide viewport whose vb is 1000 wide => 100 user units left
    const next = panViewBox(vb, VIEWPORT, 100, 50);
    expect(next.x).toBeCloseTo(-100); // 100/1000 * 1000
    expect(next.y).toBeCloseTo(-50); // 50/800 * 800
    expect(next.w).toBe(1000);
  });
});
