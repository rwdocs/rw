import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { flushSync } from "svelte";
import { render, fireEvent } from "@testing-library/svelte";
import Harness from "./__fixtures__/PopoverHarness.svelte";
import { MockResizeObserver, mockRect } from "./__fixtures__/overlay-testing";

// The panel is the immediate parent of the <span data-testid="pp-body">
// tested content — getByTestId returns the span, so stepping up one level
// lands on the fixed-position div that Popover renders.
function panelOf(body: HTMLElement): HTMLElement {
  return body.parentElement!;
}

function anchor(): HTMLElement {
  const el = document.createElement("div");
  mockRect(el, { top: 0, left: 0, width: 10, height: 10 });
  return el;
}

describe("Popover", () => {
  beforeEach(() => {
    vi.stubGlobal("ResizeObserver", MockResizeObserver);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  describe("anchored mode", () => {
    it("renders nothing when open is false", () => {
      const { queryByTestId } = render(Harness, { anchorEl: anchor(), initialOpen: false });
      expect(queryByTestId("pp-body")).toBeNull();
    });

    it("applies fixed, z-dropdown, and the extra class on the panel", () => {
      const { getByTestId } = render(Harness, {
        anchorEl: anchor(),
        initialOpen: true,
        class: "custom-marker",
      });
      const panel = panelOf(getByTestId("pp-body"));
      expect(panel.className).toContain("fixed");
      expect(panel.className).toContain("z-dropdown");
      expect(panel.className).toContain("custom-marker");
    });

    it("positions below the anchor, left edges aligned, by default", () => {
      const anchorEl = document.createElement("div");
      mockRect(anchorEl, { top: 50, left: 80, width: 120, height: 30 });

      const { getByTestId } = render(Harness, { anchorEl, initialOpen: true });
      const panel = panelOf(getByTestId("pp-body"));

      // default offset = 4: top = 50 + 30 + 4 = 84, left = 80
      expect(panel.getAttribute("style")).toContain("top: 84px");
      expect(panel.getAttribute("style")).toContain("left: 80px");
      expect(panel.getAttribute("style")).not.toContain("transform");
    });

    it("right-aligns the panel with the anchor for align=end", () => {
      const anchorEl = document.createElement("div");
      mockRect(anchorEl, { top: 10, left: 50, width: 40, height: 20 });

      const { getByTestId } = render(Harness, {
        anchorEl,
        align: "end",
        offset: 4,
        initialOpen: true,
      });
      const panel = panelOf(getByTestId("pp-body"));

      // top = 10+20+4 = 34, left = anchor.right = 90, translateX(-100%)
      expect(panel.getAttribute("style")).toContain("top: 34px");
      expect(panel.getAttribute("style")).toContain("left: 90px");
      expect(panel.getAttribute("style")).toContain("translateX(-100%)");
    });
  });

  describe("trigger mode", () => {
    it("renders the trigger snippet while closed", () => {
      const { getByTestId, queryByTestId } = render(Harness, {
        triggerLabel: "Open",
        initialOpen: false,
      });
      expect(getByTestId("pp-trigger")).toBeTruthy();
      expect(queryByTestId("pp-body")).toBeNull();
    });

    it("renders the panel below the trigger wrapper when open", () => {
      // Stub the trigger wrapper's bounding rect via a prototype mock. The
      // wrapper is <span class="inline-block"> inserted by Popover itself;
      // we override getBoundingClientRect on HTMLSpanElement.prototype for
      // the duration of this test so Popover sees deterministic coords.
      // Saved to restore afterwards, so the unbound reference is the point.
      // eslint-disable-next-line vitest/unbound-method
      const original = HTMLElement.prototype.getBoundingClientRect;
      HTMLElement.prototype.getBoundingClientRect = function () {
        if (this.tagName === "SPAN" && this.classList.contains("inline-block")) {
          return {
            top: 10,
            left: 20,
            width: 50,
            height: 14,
            right: 70,
            bottom: 24,
            x: 20,
            y: 10,
            toJSON: () => ({}),
          };
        }
        return original.call(this);
      };

      try {
        const { getByTestId } = render(Harness, {
          triggerLabel: "Open",
          initialOpen: true,
        });
        const panel = panelOf(getByTestId("pp-body"));
        // bottom: top = 10 + 14 + 4 = 28, left = 20
        expect(panel.getAttribute("style")).toContain("top: 28px");
        expect(panel.getAttribute("style")).toContain("left: 20px");
      } finally {
        HTMLElement.prototype.getBoundingClientRect = original;
      }
    });
  });

  describe("dismissal", () => {
    it("closes on Escape", async () => {
      const { getByTestId } = render(Harness, {
        anchorEl: anchor(),
        initialOpen: true,
      });
      expect(getByTestId("pp-body")).toBeTruthy();

      await fireEvent.keyDown(window, { key: "Escape" });
      flushSync();

      expect(getByTestId("pp-harness").dataset.open).toBe("false");
    });

    it("closes on outside click", async () => {
      const outside = document.createElement("button");
      outside.type = "button";
      document.body.appendChild(outside);

      try {
        const { getByTestId } = render(Harness, {
          anchorEl: anchor(),
          initialOpen: true,
        });

        await fireEvent.click(outside);
        flushSync();

        expect(getByTestId("pp-harness").dataset.open).toBe("false");
      } finally {
        outside.remove();
      }
    });

    it("does not close when the click is inside the panel", async () => {
      const { getByTestId } = render(Harness, {
        anchorEl: anchor(),
        initialOpen: true,
      });

      await fireEvent.click(getByTestId("pp-body"));
      flushSync();

      expect(getByTestId("pp-harness").dataset.open).toBe("true");
    });

    it("does not close when the click is inside the anchorEl", async () => {
      const anchorEl = document.createElement("button");
      anchorEl.type = "button";
      mockRect(anchorEl, { top: 0, left: 0, width: 10, height: 10 });
      document.body.appendChild(anchorEl);

      try {
        const { getByTestId } = render(Harness, {
          anchorEl,
          initialOpen: true,
        });

        await fireEvent.click(anchorEl);
        flushSync();

        expect(getByTestId("pp-harness").dataset.open).toBe("true");
      } finally {
        anchorEl.remove();
      }
    });

    it("removes the outside-click listener when the panel closes", async () => {
      const addSpy = vi.spyOn(document, "addEventListener");
      const removeSpy = vi.spyOn(document, "removeEventListener");

      try {
        const { getByTestId } = render(Harness, {
          anchorEl: anchor(),
          initialOpen: true,
        });
        flushSync();

        // The dismiss attachment installs a capture-phase click listener
        // while the panel is open.
        const clickCall = addSpy.mock.calls.find(
          (call) =>
            call[0] === "click" && (call[2] as AddEventListenerOptions | undefined)?.capture,
        );
        expect(clickCall).toBeTruthy();

        await fireEvent.keyDown(window, { key: "Escape" });
        flushSync();
        expect(getByTestId("pp-harness").dataset.open).toBe("false");

        // Closing unmounts the panel, so the attachment teardown ran and
        // removed the exact listener it installed.
        expect(removeSpy).toHaveBeenCalledWith("click", clickCall![1], clickCall![2]);
      } finally {
        addSpy.mockRestore();
        removeSpy.mockRestore();
      }
    });
  });

  describe("focus management", () => {
    it("restores focus to the element that had it when the panel opened", async () => {
      const { getByTestId } = render(Harness, {
        triggerLabel: "open",
        initialOpen: false,
      });

      const trigger = getByTestId("pp-trigger") as HTMLButtonElement;
      trigger.focus();
      expect(document.activeElement).toBe(trigger);

      await fireEvent.click(trigger);
      flushSync();
      expect(getByTestId("pp-harness").dataset.open).toBe("true");

      // Simulate the panel taking focus (e.g. user tabs to a link inside).
      // Escape should still return focus to the trigger, not stay on the body.
      getByTestId("pp-body").focus();

      await fireEvent.keyDown(window, { key: "Escape" });
      flushSync();

      expect(getByTestId("pp-harness").dataset.open).toBe("false");
      expect(document.activeElement).toBe(trigger);
    });

    it("does not restore focus on outside-click dismiss", async () => {
      const outside = document.createElement("button");
      outside.type = "button";
      outside.textContent = "outside";
      document.body.appendChild(outside);

      try {
        const { getByTestId } = render(Harness, {
          triggerLabel: "open",
          initialOpen: false,
        });

        const trigger = getByTestId("pp-trigger") as HTMLButtonElement;
        trigger.focus();
        await fireEvent.click(trigger);
        flushSync();

        outside.focus();
        await fireEvent.click(outside);
        flushSync();

        expect(getByTestId("pp-harness").dataset.open).toBe("false");
        // Outside-click leaves focus wherever the click landed — here, `outside`.
        // The Popover must not steal it back to the trigger.
        expect(document.activeElement).toBe(outside);
      } finally {
        outside.remove();
      }
    });
  });

  describe("aria wiring", () => {
    it("assigns a stable id to the panel", () => {
      const { getByTestId } = render(Harness, { anchorEl: anchor(), initialOpen: true });
      const panel = panelOf(getByTestId("pp-body"));
      expect(panel.id.length).toBeGreaterThan(0);
    });

    it("gives the trigger aria-controls pointing at the panel id", () => {
      const { getByTestId } = render(Harness, { triggerLabel: "Open", initialOpen: true });
      const trigger = getByTestId("pp-trigger");
      const panel = panelOf(getByTestId("pp-body"));
      expect(panel.id).toMatch(/.+/);
      expect(trigger.getAttribute("aria-controls")).toBe(panel.id);
    });

    it("sets aria-expanded on the trigger to match open state", async () => {
      const { getByTestId } = render(Harness, { triggerLabel: "Open", initialOpen: false });
      const trigger = getByTestId("pp-trigger");
      expect(trigger.getAttribute("aria-expanded")).toBe("false");

      await fireEvent.click(trigger);
      flushSync();

      expect(trigger.getAttribute("aria-expanded")).toBe("true");
    });
  });

  describe("first-paint positioning state", () => {
    it("renders the panel visible once the anchor has been measured", () => {
      const anchorEl = document.createElement("div");
      mockRect(anchorEl, { top: 50, left: 80, width: 120, height: 30 });

      const { getByTestId } = render(Harness, { anchorEl, initialOpen: true });
      const panel = panelOf(getByTestId("pp-body"));
      expect(panel.getAttribute("style")).not.toContain("visibility");
    });

    it("renders the panel visible once the trigger wrapper has been measured", () => {
      // Saved to restore afterwards, so the unbound reference is the point.
      // eslint-disable-next-line vitest/unbound-method
      const original = HTMLElement.prototype.getBoundingClientRect;
      HTMLElement.prototype.getBoundingClientRect = function () {
        if (this.tagName === "SPAN" && this.classList.contains("inline-block")) {
          return {
            top: 10,
            left: 20,
            width: 50,
            height: 14,
            right: 70,
            bottom: 24,
            x: 20,
            y: 10,
            toJSON: () => ({}),
          };
        }
        return original.call(this);
      };

      try {
        const { getByTestId } = render(Harness, { triggerLabel: "Open", initialOpen: true });
        const panel = panelOf(getByTestId("pp-body"));
        expect(panel.getAttribute("style")).not.toContain("visibility");
      } finally {
        HTMLElement.prototype.getBoundingClientRect = original;
      }
    });
  });

  describe("mode validation", () => {
    it("throws when trigger and anchorEl are both provided", () => {
      const anchorEl = document.createElement("div");
      mockRect(anchorEl, { top: 0, left: 0, width: 0, height: 0 });
      expect(() =>
        render(Harness, {
          triggerLabel: "T",
          anchorEl,
          initialOpen: true,
        }),
      ).toThrow(/exactly one of/);
    });

    it("throws when neither trigger nor anchorEl is provided", () => {
      expect(() => render(Harness, { initialOpen: true })).toThrow(/exactly one of/);
    });
  });
});
