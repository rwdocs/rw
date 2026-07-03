import { describe, it, expect } from "vitest";
import { naturalSizeOf } from "./naturalSize";

const SVG_NS = "http://www.w3.org/2000/svg";

function makeSvg(attrs: Record<string, string>): SVGSVGElement {
  const svg = document.createElementNS(SVG_NS, "svg");
  for (const [k, v] of Object.entries(attrs)) svg.setAttribute(k, v);
  return svg as SVGSVGElement;
}

describe("naturalSizeOf (svg)", () => {
  it("reads px width/height attributes", () => {
    expect(naturalSizeOf(makeSvg({ width: "300", height: "150" }))).toEqual({ w: 300, h: 150 });
  });

  it("strips a px unit suffix", () => {
    expect(naturalSizeOf(makeSvg({ width: "300px", height: "150px" }))).toEqual({
      w: 300,
      h: 150,
    });
  });

  it("falls back to viewBox when width/height are missing or non-px", () => {
    expect(naturalSizeOf(makeSvg({ viewBox: "0 0 800 400" }))).toEqual({ w: 800, h: 400 });
    // percentage width is not usable -> viewBox wins
    expect(naturalSizeOf(makeSvg({ width: "100%", viewBox: "0 0 640 480" }))).toEqual({
      w: 640,
      h: 480,
    });
  });

  it("falls back to 1x1 when nothing is available", () => {
    expect(naturalSizeOf(makeSvg({}))).toEqual({ w: 1, h: 1 });
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
