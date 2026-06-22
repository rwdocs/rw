import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { restoreFocusToThread, focusReplyTextarea } from "./focus";

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

describe("focusReplyTextarea", () => {
  // Capture the rAF callbacks instead of running them synchronously, so a test
  // can assert the focus is *deferred* (not applied before the frame fires).
  let rafCallbacks: FrameRequestCallback[] = [];
  let rafSpy: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    rafCallbacks = [];
    rafSpy = vi
      .spyOn(globalThis, "requestAnimationFrame")
      .mockImplementation((cb: FrameRequestCallback) => {
        rafCallbacks.push(cb);
        return rafCallbacks.length;
      });
  });

  afterEach(() => {
    document.body.innerHTML = "";
    rafSpy.mockRestore();
  });

  /** Run every rAF callback queued so far (one animation frame). */
  function flushRaf() {
    const cbs = rafCallbacks;
    rafCallbacks = [];
    cbs.forEach((cb) => cb(0));
  }

  function visibleTextarea(): HTMLTextAreaElement {
    const ta = document.createElement("textarea");
    document.body.appendChild(ta);
    // jsdom has no layout, so offsetParent is null for everything; force a
    // non-null value so the visibility guard treats it as on-screen.
    Object.defineProperty(ta, "offsetParent", { value: document.body, configurable: true });
    // jsdom does not implement scrollIntoView; stub it so the helper can run.
    ta.scrollIntoView = vi.fn();
    return ta;
  }

  it("focuses a visible reply textarea and scrolls it into view", () => {
    const ta = visibleTextarea();
    focusReplyTextarea(ta);
    flushRaf();
    expect(document.activeElement).toBe(ta);
    expect(ta.scrollIntoView).toHaveBeenCalledWith({ block: "nearest" });
  });

  it("defers the focus to the next animation frame", () => {
    const ta = visibleTextarea();
    focusReplyTextarea(ta);
    // Before the frame fires, focus must not have moved — the deferral exists so
    // a sidebar card that is visibility:hidden until measured has flipped to
    // visible by the time we focus.
    expect(document.activeElement).not.toBe(ta);
    flushRaf();
    expect(document.activeElement).toBe(ta);
  });

  it("does nothing when the textarea is null", () => {
    expect(() => focusReplyTextarea(null)).not.toThrow();
    flushRaf();
    expect(document.activeElement).not.toBeInstanceOf(HTMLTextAreaElement);
  });

  it("does not focus a textarea hidden in a display:none subtree", () => {
    const ta = document.createElement("textarea");
    ta.scrollIntoView = vi.fn();
    document.body.appendChild(ta);
    // offsetParent stays null (jsdom default) → treated as display:none.
    focusReplyTextarea(ta);
    flushRaf();
    expect(document.activeElement).not.toBe(ta);
    expect(ta.scrollIntoView).not.toHaveBeenCalled();
  });
});
