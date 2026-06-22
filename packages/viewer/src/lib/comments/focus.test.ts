import { describe, it, expect, afterEach } from "vitest";
import { restoreFocusToThread } from "./focus";

afterEach(() => {
  document.body.innerHTML = "";
});

describe("restoreFocusToThread", () => {
  it("focuses the given thread element", () => {
    const card = document.createElement("div");
    card.tabIndex = -1;
    document.body.appendChild(card);

    restoreFocusToThread(card);

    expect(document.activeElement).toBe(card);
  });

  it("blurs the active editable element when no target is given", () => {
    const ta = document.createElement("textarea");
    document.body.appendChild(ta);
    ta.focus();
    expect(document.activeElement).toBe(ta);

    restoreFocusToThread(null);

    expect(document.activeElement).not.toBe(ta);
  });

  it("does nothing harmful when target is null and nothing is focused", () => {
    expect(() => restoreFocusToThread(undefined)).not.toThrow();
  });

  it("leaves a focused non-textarea element untouched (only releases the composer)", () => {
    const btn = document.createElement("button");
    document.body.appendChild(btn);
    btn.focus();
    expect(document.activeElement).toBe(btn);

    restoreFocusToThread(null);

    // The fallback only blurs the composer textarea — focus that legitimately
    // moved elsewhere (here a button) must not be yanked away.
    expect(document.activeElement).toBe(btn);
  });
});
