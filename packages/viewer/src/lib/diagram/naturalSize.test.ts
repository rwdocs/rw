import { describe, it, expect } from "vitest";
import { naturalSizeOf } from "./naturalSize";

const SVG_NS = "http://www.w3.org/2000/svg";

function makeSvg(attrs: Record<string, string>): SVGSVGElement {
  const svg = document.createElementNS(SVG_NS, "svg");
  for (const [k, v] of Object.entries(attrs)) svg.setAttribute(k, v);
  return svg;
}

describe("naturalSizeOf (svg)", () => {
  it.each([
    ["reads px width/height", { width: "300", height: "150" }, { w: 300, h: 150 }],
    ["strips a px unit suffix", { width: "300px", height: "150px" }, { w: 300, h: 150 }],
    ["accepts fractional lengths", { width: "300.5", height: "150.25px" }, { w: 300.5, h: 150.25 }],
    ["accepts a missing leading digit", { width: ".5", height: ".25px" }, { w: 0.5, h: 0.25 }],
    [
      "rejects a trailing decimal point, falling back to viewBox",
      { width: "300.", height: "150.", viewBox: "0 0 640 480" },
      { w: 640, h: 480 },
    ],
    ["falls back to viewBox without width/height", { viewBox: "0 0 800 400" }, { w: 800, h: 400 }],
    [
      "falls back to viewBox for a percentage width",
      { width: "100%", viewBox: "0 0 640 480" },
      { w: 640, h: 480 },
    ],
    ["falls back to 1x1 when nothing is available", {}, { w: 1, h: 1 }],
  ])("%s", (_, attrs: Record<string, string>, size) => {
    expect(naturalSizeOf(makeSvg(attrs))).toEqual(size);
  });
});

describe("naturalSizeOf (img)", () => {
  it("prefers naturalWidth/naturalHeight", () => {
    const img = document.createElement("img");
    Object.defineProperty(img, "naturalWidth", { value: 500 });
    Object.defineProperty(img, "naturalHeight", { value: 250 });
    expect(naturalSizeOf(img)).toEqual({ w: 500, h: 250 });
  });

  it("falls back to width/height attributes when natural size is 0 (not yet loaded)", () => {
    const img = document.createElement("img");
    Object.defineProperty(img, "naturalWidth", { value: 0 });
    Object.defineProperty(img, "naturalHeight", { value: 0 });
    img.setAttribute("width", "200");
    img.setAttribute("height", "120");
    expect(naturalSizeOf(img)).toEqual({ w: 200, h: 120 });
  });
});
