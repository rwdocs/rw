import { describe, expect, it } from "vitest";
import { DIAGRAM_ARTICLE_CSS, DIAGRAM_MODAL_CSS, applySheet, sheetFor } from "./sheet";

describe("diagram sheets", () => {
  it("keeps the article's auto height out of the modal sheet", () => {
    // A stylesheet `!important` beats the modal's inline height, so the
    // article's sizing must never reach the popup.
    expect(DIAGRAM_ARTICLE_CSS).toContain("height: auto !important");
    expect(DIAGRAM_MODAL_CSS).not.toContain("height: auto");
    expect(DIAGRAM_MODAL_CSS).toContain("height: 100%");
  });

  it("carries no theme-dependent rule (a shadow root cannot see .dark)", () => {
    expect(DIAGRAM_ARTICLE_CSS).not.toContain(".dark");
    expect(DIAGRAM_MODAL_CSS).not.toContain(".dark");
    expect(DIAGRAM_ARTICLE_CSS).not.toContain("invert(");
    expect(DIAGRAM_MODAL_CSS).not.toContain("invert(");
  });

  it("hands the same CSS string one shared sheet", () => {
    // Two roots given the same CSS adopt one object rather than a sheet each.
    // Exercised through the memo directly: jsdom has no adoptedStyleSheets, so
    // applySheet can never reach the constructable path here.
    const article = sheetFor(DIAGRAM_ARTICLE_CSS);
    expect(article).not.toBeNull();
    expect(sheetFor(DIAGRAM_ARTICLE_CSS)).toBe(article);
    expect(sheetFor(DIAGRAM_MODAL_CSS)).not.toBe(article);
  });

  it("falls back to a <style> element where adoptedStyleSheets is unsupported", () => {
    const root = document.createElement("div").attachShadow({ mode: "open" });
    applySheet(root, "svg { color: red; }");
    // jsdom has no adoptedStyleSheets, so this must have appended a <style>.
    const style = root.querySelector("style");
    expect(style?.textContent).toBe("svg { color: red; }");
  });
});
