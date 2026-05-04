import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { flushSync } from "svelte";
import { render, fireEvent } from "@testing-library/svelte";
import Harness from "./__fixtures__/PopoverHarness.svelte";
import { MockResizeObserver, fakeRect, mockRect } from "./__fixtures__/overlay-testing";

// The panel is the immediate parent of the <span data-testid="pp-body">
// tested content — getByTestId returns the span, so stepping up one level
// lands on the fixed-position div that Popover renders.
function panelOf(body: HTMLElement): HTMLElement {
  return body.parentElement!;
}

describe("Popover", () => {
  beforeEach(() => {
    vi.stubGlobal("ResizeObserver", MockResizeObserver);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  describe("free mode", () => {
    it("renders nothing when open is false", () => {
      const { queryByTestId } = render(Harness, {
        x: 10,
        y: 20,
        initialOpen: false,
      });
      expect(queryByTestId("pp-body")).toBeNull();
    });

    it("renders the panel when open is true", () => {
      const { getByTestId } = render(Harness, {
        x: 10,
        y: 20,
        initialOpen: true,
      });
      expect(getByTestId("pp-body")).toBeTruthy();
    });

    it("positions the panel at raw x/y", () => {
      const { getByTestId } = render(Harness, {
        x: 100,
        y: 200,
        initialOpen: true,
      });
      const panel = panelOf(getByTestId("pp-body"));
      expect(panel.getAttribute("style")).toContain("top: 200px");
      expect(panel.getAttribute("style")).toContain("left: 100px");
      expect(panel.getAttribute("style")).not.toContain("transform");
    });

    it("applies z-dropdown, fixed, and the extra class on the panel", () => {
      const { getByTestId } = render(Harness, {
        x: 0,
        y: 0,
        initialOpen: true,
        class: "custom-marker",
      });
      const panel = panelOf(getByTestId("pp-body"));
      expect(panel.className).toContain("fixed");
      expect(panel.className).toContain("z-dropdown");
      expect(panel.className).toContain("custom-marker");
    });
  });

  describe("anchored mode", () => {
    it("positions below the anchor by default (bottom placement)", () => {
      const anchorEl = document.createElement("div");
      mockRect(anchorEl, { top: 50, left: 80, width: 120, height: 30 });

      const { getByTestId } = render(Harness, { anchorEl, initialOpen: true });
      const panel = panelOf(getByTestId("pp-body"));

      // default offset = 4; bottom placement: top = 50 + 30 + 4 = 84, left = 80
      expect(panel.getAttribute("style")).toContain("top: 84px");
      expect(panel.getAttribute("style")).toContain("left: 80px");
      expect(panel.getAttribute("style")).not.toContain("transform");
    });

    it("shifts above the anchor for placement=top via transform", () => {
      const anchorEl = document.createElement("div");
      mockRect(anchorEl, { top: 100, left: 50, width: 20, height: 10 });

      const { getByTestId } = render(Harness, {
        anchorEl,
        placement: "top",
        offset: 8,
        initialOpen: true,
      });
      const panel = panelOf(getByTestId("pp-body"));

      // top placement: top = anchor.top - offset = 92; transform translateY(-100%)
      expect(panel.getAttribute("style")).toContain("top: 92px");
      expect(panel.getAttribute("style")).toContain("translateY(-100%)");
    });

    it("positions to the right of the anchor for placement=right", () => {
      const anchorEl = document.createElement("div");
      mockRect(anchorEl, { top: 40, left: 100, width: 60, height: 20 });

      const { getByTestId } = render(Harness, {
        anchorEl,
        placement: "right",
        offset: 4,
        initialOpen: true,
      });
      const panel = panelOf(getByTestId("pp-body"));

      // right: left = 100 + 60 + 4 = 164, top = 40
      expect(panel.getAttribute("style")).toContain("left: 164px");
      expect(panel.getAttribute("style")).toContain("top: 40px");
    });

    it("shifts left of the anchor for placement=left via transform", () => {
      const anchorEl = document.createElement("div");
      mockRect(anchorEl, { top: 40, left: 100, width: 60, height: 20 });

      const { getByTestId } = render(Harness, {
        anchorEl,
        placement: "left",
        offset: 0,
        initialOpen: true,
      });
      const panel = panelOf(getByTestId("pp-body"));

      expect(panel.getAttribute("style")).toContain("left: 100px");
      expect(panel.getAttribute("style")).toContain("translateX(-100%)");
    });

    it("right-aligns the panel with the anchor for placement=bottom align=end", () => {
      const anchorEl = document.createElement("div");
      mockRect(anchorEl, { top: 10, left: 50, width: 40, height: 20 });

      const { getByTestId } = render(Harness, {
        anchorEl,
        placement: "bottom",
        align: "end",
        offset: 4,
        initialOpen: true,
      });
      const panel = panelOf(getByTestId("pp-body"));

      // bottom+end: top = 10+20+4 = 34, left = anchor.right = 90, translateX(-100%)
      expect(panel.getAttribute("style")).toContain("top: 34px");
      expect(panel.getAttribute("style")).toContain("left: 90px");
      expect(panel.getAttribute("style")).toContain("translateX(-100%)");
    });

    it("bottom-aligns the panel for placement=right align=end", () => {
      const anchorEl = document.createElement("div");
      mockRect(anchorEl, { top: 10, left: 50, width: 40, height: 20 });

      const { getByTestId } = render(Harness, {
        anchorEl,
        placement: "right",
        align: "end",
        offset: 4,
        initialOpen: true,
      });
      const panel = panelOf(getByTestId("pp-body"));

      // right+end: left = 50+40+4 = 94, top = anchor.bottom = 30, translateY(-100%)
      expect(panel.getAttribute("style")).toContain("left: 94px");
      expect(panel.getAttribute("style")).toContain("top: 30px");
      expect(panel.getAttribute("style")).toContain("translateY(-100%)");
    });

    it("combines both translates for placement=top align=end", () => {
      const anchorEl = document.createElement("div");
      mockRect(anchorEl, { top: 100, left: 50, width: 40, height: 20 });

      const { getByTestId } = render(Harness, {
        anchorEl,
        placement: "top",
        align: "end",
        offset: 0,
        initialOpen: true,
      });
      const panel = panelOf(getByTestId("pp-body"));

      // top+end: top = 100, left = 90, translateY(-100%) translateX(-100%)
      expect(panel.getAttribute("style")).toContain("top: 100px");
      expect(panel.getAttribute("style")).toContain("left: 90px");
      expect(panel.getAttribute("style")).toContain("translateY(-100%)");
      expect(panel.getAttribute("style")).toContain("translateX(-100%)");
    });
  });

  describe("trigger mode", () => {
    it("renders the trigger snippet whether open is true or false", () => {
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
      const original = HTMLElement.prototype.getBoundingClientRect;
      HTMLElement.prototype.getBoundingClientRect = function () {
        if (this.tagName === "SPAN" && this.classList.contains("inline-block")) {
          return fakeRect({
            top: 10,
            left: 20,
            width: 50,
            height: 14,
            right: 70,
            bottom: 24,
            x: 20,
            y: 10,
          });
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

  describe("dismissible", () => {
    it("closes on Escape when dismissible is true", async () => {
      const { getByTestId } = render(Harness, {
        x: 0,
        y: 0,
        initialOpen: true,
        dismissible: true,
      });
      expect(getByTestId("pp-body")).toBeTruthy();

      await fireEvent.keyDown(window, { key: "Escape" });
      flushSync();

      expect(getByTestId("pp-harness").dataset.open).toBe("false");
    });

    it("ignores Escape when dismissible is false", async () => {
      const { getByTestId, queryByTestId } = render(Harness, {
        x: 0,
        y: 0,
        initialOpen: true,
        dismissible: false,
      });

      await fireEvent.keyDown(window, { key: "Escape" });
      flushSync();

      expect(getByTestId("pp-harness").dataset.open).toBe("true");
      expect(queryByTestId("pp-body")).toBeTruthy();
    });

    it("closes on outside click when dismissible is true", async () => {
      const outside = document.createElement("button");
      outside.type = "button";
      document.body.appendChild(outside);

      try {
        const { getByTestId } = render(Harness, {
          x: 0,
          y: 0,
          initialOpen: true,
          dismissible: true,
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
        x: 0,
        y: 0,
        initialOpen: true,
        dismissible: true,
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
          dismissible: true,
        });

        await fireEvent.click(anchorEl);
        flushSync();

        expect(getByTestId("pp-harness").dataset.open).toBe("true");
      } finally {
        anchorEl.remove();
      }
    });
  });

  describe("aria", () => {
    it("applies the role attribute to the panel when provided", () => {
      const { getByTestId } = render(Harness, {
        x: 0,
        y: 0,
        initialOpen: true,
        role: "menu",
      });
      const panel = panelOf(getByTestId("pp-body"));
      expect(panel.getAttribute("role")).toBe("menu");
    });

    it("omits the role attribute when not provided", () => {
      const { getByTestId } = render(Harness, {
        x: 0,
        y: 0,
        initialOpen: true,
      });
      const panel = panelOf(getByTestId("pp-body"));
      expect(panel.hasAttribute("role")).toBe(false);
    });
  });

  describe("focus management", () => {
    it("restores focus to the element that had it when the panel opened", async () => {
      const { getByTestId } = render(Harness, {
        triggerLabel: "open",
        initialOpen: false,
        dismissible: true,
      });

      const trigger = getByTestId("pp-trigger") as HTMLButtonElement;
      trigger.focus();
      expect(document.activeElement).toBe(trigger);

      await fireEvent.click(trigger);
      flushSync();
      expect(getByTestId("pp-harness").dataset.open).toBe("true");

      // Simulate the panel taking focus (e.g. user tabs to a link inside).
      // Escape should still return focus to the trigger, not stay on the body.
      (getByTestId("pp-body") as HTMLElement).focus();

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
          dismissible: true,
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
    it("assigns a stable id to the panel in every mode", () => {
      const { getByTestId } = render(Harness, { x: 0, y: 0, initialOpen: true });
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

    it("sets aria-haspopup on the trigger for menu/listbox/dialog roles", () => {
      const { getByTestId, unmount } = render(Harness, {
        triggerLabel: "Open",
        initialOpen: true,
        role: "menu",
      });
      expect(getByTestId("pp-trigger").getAttribute("aria-haspopup")).toBe("menu");
      unmount();

      const { getByTestId: getByTestId2 } = render(Harness, {
        triggerLabel: "Open",
        initialOpen: true,
        role: "listbox",
      });
      expect(getByTestId2("pp-trigger").getAttribute("aria-haspopup")).toBe("listbox");
    });

    it("omits aria-haspopup when no role or a non-popup role is set", () => {
      const { getByTestId } = render(Harness, {
        triggerLabel: "Open",
        initialOpen: true,
      });
      const trigger = getByTestId("pp-trigger");
      expect(trigger.hasAttribute("aria-haspopup")).toBe(false);
      expect(trigger.getAttribute("aria-controls")).toBeTruthy();
    });

    it("wires aria-describedby (not aria-controls/expanded/haspopup) for role=tooltip", () => {
      const { getByTestId } = render(Harness, {
        triggerLabel: "Info",
        initialOpen: true,
        role: "tooltip",
      });
      const trigger = getByTestId("pp-trigger");
      const panel = panelOf(getByTestId("pp-body"));
      expect(trigger.getAttribute("aria-describedby")).toBe(panel.id);
      expect(trigger.hasAttribute("aria-controls")).toBe(false);
      expect(trigger.hasAttribute("aria-expanded")).toBe(false);
      expect(trigger.hasAttribute("aria-haspopup")).toBe(false);
    });
  });

  describe("first-paint positioning state", () => {
    it("renders the panel visible immediately in free mode", () => {
      const { getByTestId } = render(Harness, { x: 10, y: 20, initialOpen: true });
      const panel = panelOf(getByTestId("pp-body"));
      expect(panel.getAttribute("style")).not.toContain("visibility");
      expect(panel.getAttribute("style")).not.toContain("pointer-events");
    });

    it("renders the panel visible once the anchor has been measured", () => {
      const anchorEl = document.createElement("div");
      mockRect(anchorEl, { top: 50, left: 80, width: 120, height: 30 });

      const { getByTestId } = render(Harness, { anchorEl, initialOpen: true });
      const panel = panelOf(getByTestId("pp-body"));
      expect(panel.getAttribute("style")).not.toContain("visibility");
    });

    it("renders the panel visible once the trigger wrapper has been measured", () => {
      const original = HTMLElement.prototype.getBoundingClientRect;
      HTMLElement.prototype.getBoundingClientRect = function () {
        if (this.tagName === "SPAN" && this.classList.contains("inline-block")) {
          return fakeRect({
            top: 10,
            left: 20,
            width: 50,
            height: 14,
            right: 70,
            bottom: 24,
            x: 20,
            y: 10,
          });
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
      ).toThrow(/mutually exclusive/);
    });

    it("throws when anchorEl and x/y are both provided", () => {
      const anchorEl = document.createElement("div");
      mockRect(anchorEl, { top: 0, left: 0, width: 0, height: 0 });
      expect(() =>
        render(Harness, {
          anchorEl,
          x: 0,
          y: 0,
          initialOpen: true,
        }),
      ).toThrow(/mutually exclusive/);
    });

    it("throws when only x or only y is provided", () => {
      expect(() => render(Harness, { x: 0, initialOpen: true })).toThrow(
        /must be provided together/,
      );
    });

    it("throws when no mode is specified", () => {
      expect(() => render(Harness, { initialOpen: true })).toThrow(/specify one of/);
    });
  });
});
