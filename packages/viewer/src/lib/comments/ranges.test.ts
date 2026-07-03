import { describe, it, expect, afterEach } from "vitest";
import { rangesOverlap, rangeIntersectsNode } from "./ranges";

afterEach(() => {
  document.body.innerHTML = "";
});

function rangeOverText(el: Element): Range {
  const r = document.createRange();
  r.selectNodeContents(el.firstChild as Text);
  return r;
}

describe("ranges", () => {
  it("rangesOverlap is true for overlapping interiors, false for merely touching", () => {
    document.body.innerHTML = `<p id="a">hello</p>`;
    const t = document.getElementById("a")!.firstChild as Text;
    const a = document.createRange();
    a.setStart(t, 0);
    a.setEnd(t, 3); // "hel"
    const b = document.createRange();
    b.setStart(t, 2);
    b.setEnd(t, 5); // "llo" — overlaps "l"
    expect(rangesOverlap(a, b)).toBe(true);

    const c = document.createRange();
    c.setStart(t, 3);
    c.setEnd(t, 5); // "lo" — only touches a's end
    expect(rangesOverlap(a, c)).toBe(false);
  });

  it("rangeIntersectsNode is true when the range covers the node", () => {
    document.body.innerHTML = `<div><span id="s">x</span></div>`;
    const span = document.getElementById("s")!;
    const range = rangeOverText(span);
    expect(rangeIntersectsNode(range, span)).toBe(true);
  });
});
