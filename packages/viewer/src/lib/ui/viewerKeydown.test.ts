import { describe, it, expect, vi, afterEach } from "vitest";
import { onViewerKeydown } from "./viewerKeydown";

function press(target: EventTarget) {
  target.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
}

describe("onViewerKeydown", () => {
  afterEach(() => {
    document.body.innerHTML = "";
  });

  it("hears keys from anywhere when the viewer is standalone", () => {
    const overlay = document.body.appendChild(document.createElement("div"));
    const outside = document.body.appendChild(document.createElement("input"));
    const handler = vi.fn();

    const off = onViewerKeydown(overlay, handler);
    press(outside);
    press(window);
    off();
    press(outside);

    expect(handler).toHaveBeenCalledTimes(2);
  });

  it("hears only keys from inside the viewer when mounted in a host", () => {
    const root = document.body.appendChild(document.createElement("div"));
    root.setAttribute("data-rw-viewer", "");
    const trigger = root.appendChild(document.createElement("button"));
    const overlay = root.appendChild(document.createElement("div"));
    const hostField = document.body.appendChild(document.createElement("input"));
    const handler = vi.fn();

    const off = onViewerKeydown(overlay, handler);
    press(hostField);
    press(trigger);
    off();
    press(trigger);

    expect(handler).toHaveBeenCalledTimes(1);
  });

  it("hears keys from a shadow root inside the viewer, such as a diagram's", () => {
    const root = document.body.appendChild(document.createElement("div"));
    root.setAttribute("data-rw-viewer", "");
    const overlay = root.appendChild(document.createElement("div"));
    const shadowHost = root.appendChild(document.createElement("div"));
    const inner = shadowHost
      .attachShadow({ mode: "open" })
      .appendChild(document.createElement("a"));
    const handler = vi.fn();

    const off = onViewerKeydown(overlay, handler);
    inner.dispatchEvent(
      new KeyboardEvent("keydown", { key: "Escape", bubbles: true, composed: true }),
    );
    off();

    expect(handler).toHaveBeenCalledTimes(1);
  });

  it("hears keys with nothing focused when mounted in a host", () => {
    const root = document.body.appendChild(document.createElement("div"));
    root.setAttribute("data-rw-viewer", "");
    const overlay = root.appendChild(document.createElement("div"));
    const handler = vi.fn();

    const off = onViewerKeydown(overlay, handler);
    press(document.body);
    off();

    expect(handler).toHaveBeenCalledTimes(1);
  });
});
