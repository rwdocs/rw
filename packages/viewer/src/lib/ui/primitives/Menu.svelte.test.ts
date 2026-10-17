import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { flushSync } from "svelte";
import { render, fireEvent } from "@testing-library/svelte";
import Harness from "./__fixtures__/MenuHarness.svelte";
import { MockResizeObserver, createAnchor } from "./__fixtures__/overlay-testing";

describe("Menu", () => {
  beforeEach(() => {
    vi.stubGlobal("ResizeObserver", MockResizeObserver);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    document.body.innerHTML = "";
  });

  describe("rendering", () => {
    it("does not render items when open is false", () => {
      const anchorEl = createAnchor();
      const { queryByRole } = render(Harness, { anchorEl, initialOpen: false });
      expect(queryByRole("menu")).toBeNull();
      expect(queryByRole("menuitem")).toBeNull();
    });

    it("forwards aria-label onto the menu container", () => {
      const anchorEl = createAnchor();
      const { getByRole } = render(Harness, {
        anchorEl,
        initialOpen: true,
        ariaLabel: "Breadcrumb jumps",
      });
      expect(getByRole("menu").getAttribute("aria-label")).toBe("Breadcrumb jumps");
    });

    it("renders each item with role=menuitem", () => {
      const anchorEl = createAnchor();
      const { getAllByRole } = render(Harness, {
        anchorEl,
        initialOpen: true,
        items: [{ label: "A" }, { label: "B" }],
      });
      expect(getAllByRole("menuitem")).toHaveLength(2);
    });

    it("renders <a> when href is set, <button> when it's not", () => {
      const anchorEl = createAnchor();
      const { getByText } = render(Harness, {
        anchorEl,
        initialOpen: true,
        items: [{ label: "Link", href: "/docs/x" }, { label: "Action" }],
      });
      expect(getByText("Link").tagName).toBe("A");
      expect(getByText("Link").getAttribute("href")).toBe("/docs/x");
      expect(getByText("Action").tagName).toBe("BUTTON");
    });
  });

  describe("auto-focus", () => {
    it("focuses the first item when the menu opens", () => {
      const anchorEl = createAnchor();
      const { getByText } = render(Harness, {
        anchorEl,
        initialOpen: true,
        items: [{ label: "First" }, { label: "Second" }],
      });
      flushSync();
      expect(document.activeElement).toBe(getByText("First"));
    });
  });

  describe("keyboard navigation", () => {
    // All tests in this block dispatch keydowns on `document.activeElement`
    // — which the auto-focus effect has pointed at the focused menuitem.
    // That matches real usage: the browser fires keydown on the focused
    // element, and Menu.Root's handler on the menu div catches it via
    // event bubbling. Dispatching directly on the menu div would pass
    // even if the bubbling path broke.
    it("ArrowDown moves focus to the next item and wraps at the end", async () => {
      const anchorEl = createAnchor();
      const { getByText } = render(Harness, {
        anchorEl,
        initialOpen: true,
        items: [{ label: "A" }, { label: "B" }, { label: "C" }],
      });
      flushSync();

      expect(document.activeElement).toBe(getByText("A"));

      await fireEvent.keyDown(document.activeElement as Element, { key: "ArrowDown" });
      expect(document.activeElement).toBe(getByText("B"));

      await fireEvent.keyDown(document.activeElement as Element, { key: "ArrowDown" });
      expect(document.activeElement).toBe(getByText("C"));

      // wrap
      await fireEvent.keyDown(document.activeElement as Element, { key: "ArrowDown" });
      expect(document.activeElement).toBe(getByText("A"));
    });

    it("ArrowUp moves focus backward and wraps at the start", async () => {
      const anchorEl = createAnchor();
      const { getByText } = render(Harness, {
        anchorEl,
        initialOpen: true,
        items: [{ label: "A" }, { label: "B" }, { label: "C" }],
      });
      flushSync();

      // wrap from first
      await fireEvent.keyDown(document.activeElement as Element, { key: "ArrowUp" });
      expect(document.activeElement).toBe(getByText("C"));

      await fireEvent.keyDown(document.activeElement as Element, { key: "ArrowUp" });
      expect(document.activeElement).toBe(getByText("B"));
    });

    it("Home jumps to the first item, End to the last", async () => {
      const anchorEl = createAnchor();
      const { getByText } = render(Harness, {
        anchorEl,
        initialOpen: true,
        items: [{ label: "A" }, { label: "B" }, { label: "C" }],
      });
      flushSync();

      await fireEvent.keyDown(document.activeElement as Element, { key: "End" });
      expect(document.activeElement).toBe(getByText("C"));

      await fireEvent.keyDown(document.activeElement as Element, { key: "Home" });
      expect(document.activeElement).toBe(getByText("A"));
    });

    it("ArrowDown prevents default so the page doesn't scroll", async () => {
      const anchorEl = createAnchor();
      const { getByText } = render(Harness, {
        anchorEl,
        initialOpen: true,
        items: [{ label: "A" }, { label: "B" }],
      });
      flushSync();
      expect(document.activeElement).toBe(getByText("A"));

      const event = new KeyboardEvent("keydown", {
        key: "ArrowDown",
        bubbles: true,
        cancelable: true,
      });
      (document.activeElement as Element).dispatchEvent(event);
      expect(event.defaultPrevented).toBe(true);
    });
  });

  describe("activation", () => {
    it("clicking an item fires onclick and closes the menu", async () => {
      const anchorEl = createAnchor();
      const onItemClick = vi.fn();
      const { getByTestId, getByText } = render(Harness, {
        anchorEl,
        initialOpen: true,
        onItemClick,
        items: [{ label: "Alpha" }, { label: "Beta" }],
      });

      await fireEvent.click(getByText("Alpha"));
      flushSync();

      expect(onItemClick).toHaveBeenCalledWith("Alpha");
      expect(getByTestId("m-harness").dataset.open).toBe("false");
    });

    it("activating an item returns focus to the anchor", async () => {
      const anchorEl = createAnchor();
      anchorEl.focus();

      const { getByText } = render(Harness, {
        anchorEl,
        initialOpen: true,
        items: [{ label: "Alpha" }, { label: "Beta" }],
      });
      flushSync();
      expect(document.activeElement).toBe(getByText("Alpha"));

      await fireEvent.click(getByText("Alpha"));
      flushSync();

      expect(document.activeElement).toBe(anchorEl);
    });
  });

  describe("dismissal", () => {
    it("Escape closes the menu", async () => {
      const anchorEl = createAnchor();
      const { getByTestId } = render(Harness, { anchorEl, initialOpen: true });

      await fireEvent.keyDown(window, { key: "Escape" });
      flushSync();

      expect(getByTestId("m-harness").dataset.open).toBe("false");
    });

    it("outside-click closes the menu", async () => {
      const anchorEl = createAnchor();
      const outside = document.createElement("button");
      outside.type = "button";
      document.body.appendChild(outside);

      const { getByTestId } = render(Harness, { anchorEl, initialOpen: true });

      await fireEvent.click(outside);
      flushSync();

      expect(getByTestId("m-harness").dataset.open).toBe("false");
    });

    it("clicking inside the menu panel does not close it (non-item area)", async () => {
      const anchorEl = createAnchor();
      const { getByTestId, getByRole } = render(Harness, {
        anchorEl,
        initialOpen: true,
      });

      await fireEvent.click(getByRole("menu"));
      flushSync();

      expect(getByTestId("m-harness").dataset.open).toBe("true");
    });

    it("Tab closes the menu and returns focus to the anchor", async () => {
      const anchorEl = createAnchor();
      anchorEl.focus();

      const { getByTestId, getByText } = render(Harness, {
        anchorEl,
        initialOpen: true,
        items: [{ label: "First" }, { label: "Second" }],
      });
      flushSync();
      expect(document.activeElement).toBe(getByText("First"));

      await fireEvent.keyDown(getByText("First"), { key: "Tab" });
      flushSync();

      expect(getByTestId("m-harness").dataset.open).toBe("false");
      expect(document.activeElement).toBe(anchorEl);
    });

    it("Shift+Tab closes the menu and returns focus to the anchor", async () => {
      const anchorEl = createAnchor();
      anchorEl.focus();

      const { getByTestId, getByText } = render(Harness, {
        anchorEl,
        initialOpen: true,
        items: [{ label: "First" }, { label: "Second" }],
      });
      flushSync();

      await fireEvent.keyDown(getByText("First"), { key: "Tab", shiftKey: true });
      flushSync();

      expect(getByTestId("m-harness").dataset.open).toBe("false");
      expect(document.activeElement).toBe(anchorEl);
    });

    it("Tab does not prevent default so native tab navigation runs", async () => {
      const anchorEl = createAnchor();
      anchorEl.focus();
      const { getByText } = render(Harness, {
        anchorEl,
        initialOpen: true,
        items: [{ label: "First" }],
      });
      flushSync();
      expect(document.activeElement).toBe(getByText("First"));

      const event = new KeyboardEvent("keydown", {
        key: "Tab",
        bubbles: true,
        cancelable: true,
      });
      (document.activeElement as Element).dispatchEvent(event);
      expect(event.defaultPrevented).toBe(false);
    });

    it("Escape returns focus to the anchor element", async () => {
      // Focus the anchor *before* mount so Popover's restoreFocusEl effect
      // (which captures document.activeElement when open becomes true) picks
      // it up. This guards the cross-component effect ordering between
      // Popover's focus capture and Menu.Root's auto-focus.
      const anchorEl = createAnchor();
      anchorEl.focus();
      expect(document.activeElement).toBe(anchorEl);

      const { getByText } = render(Harness, {
        anchorEl,
        initialOpen: true,
        items: [{ label: "First" }, { label: "Second" }],
      });
      flushSync();
      expect(document.activeElement).toBe(getByText("First"));

      await fireEvent.keyDown(window, { key: "Escape" });
      flushSync();

      expect(document.activeElement).toBe(anchorEl);
    });
  });

  describe("trigger wiring", () => {
    it("sets aria-haspopup, aria-controls, and aria-expanded on the anchor", () => {
      const anchorEl = createAnchor();
      const { getByRole } = render(Harness, { anchorEl, initialOpen: true });
      flushSync();

      expect(anchorEl.getAttribute("aria-haspopup")).toBe("menu");
      expect(anchorEl.getAttribute("aria-expanded")).toBe("true");
      const controls = anchorEl.getAttribute("aria-controls");
      expect(controls).toBeTruthy();
      expect(getByRole("menu").id).toBe(controls);
    });

    it("aria-expanded tracks open state", async () => {
      const anchorEl = createAnchor();
      render(Harness, {
        anchorEl,
        initialOpen: false,
        items: [{ label: "A" }, { label: "B" }],
      });
      flushSync();
      expect(anchorEl.getAttribute("aria-expanded")).toBe("false");

      await fireEvent.keyDown(anchorEl, { key: "ArrowDown" });
      flushSync();
      expect(anchorEl.getAttribute("aria-expanded")).toBe("true");
    });

    it("ArrowDown on the anchor opens the menu and focuses the first item", async () => {
      const anchorEl = createAnchor();
      anchorEl.focus();
      const { getByText } = render(Harness, {
        anchorEl,
        initialOpen: false,
        items: [{ label: "A" }, { label: "B" }, { label: "C" }],
      });
      flushSync();

      await fireEvent.keyDown(anchorEl, { key: "ArrowDown" });
      flushSync();

      expect(document.activeElement).toBe(getByText("A"));
    });

    it("ArrowUp on the anchor opens the menu and focuses the last item", async () => {
      const anchorEl = createAnchor();
      anchorEl.focus();
      const { getByText } = render(Harness, {
        anchorEl,
        initialOpen: false,
        items: [{ label: "A" }, { label: "B" }, { label: "C" }],
      });
      flushSync();

      await fireEvent.keyDown(anchorEl, { key: "ArrowUp" });
      flushSync();

      expect(document.activeElement).toBe(getByText("C"));
    });

    it("ArrowDown on the anchor calls preventDefault to suppress page scroll", () => {
      const anchorEl = createAnchor();
      anchorEl.focus();
      render(Harness, {
        anchorEl,
        initialOpen: false,
        items: [{ label: "A" }],
      });
      flushSync();

      const event = new KeyboardEvent("keydown", {
        key: "ArrowDown",
        bubbles: true,
        cancelable: true,
      });
      anchorEl.dispatchEvent(event);
      expect(event.defaultPrevented).toBe(true);
    });

    it("reopening after an ArrowUp uses first-item focus by default", async () => {
      const anchorEl = createAnchor();
      anchorEl.focus();
      const { getByText, getByTestId } = render(Harness, {
        anchorEl,
        initialOpen: false,
        items: [{ label: "A" }, { label: "B" }, { label: "C" }],
      });
      flushSync();

      await fireEvent.keyDown(anchorEl, { key: "ArrowUp" });
      flushSync();
      expect(document.activeElement).toBe(getByText("C"));

      // Close and reopen via a plain click-style toggle — the pending
      // "last" hint must have been consumed on the prior open.
      await fireEvent.keyDown(window, { key: "Escape" });
      flushSync();
      expect(getByTestId("m-harness").dataset.open).toBe("false");

      await fireEvent.keyDown(anchorEl, { key: "ArrowDown" });
      flushSync();
      expect(document.activeElement).toBe(getByText("A"));
    });
  });

  describe("roving tabindex", () => {
    it("on open, the first item is tabindex=0 and the rest are -1", () => {
      const anchorEl = createAnchor();
      const { getByText } = render(Harness, {
        anchorEl,
        initialOpen: true,
        items: [{ label: "A" }, { label: "B" }, { label: "C" }],
      });
      flushSync();
      expect(getByText("A").getAttribute("tabindex")).toBe("0");
      expect(getByText("B").getAttribute("tabindex")).toBe("-1");
      expect(getByText("C").getAttribute("tabindex")).toBe("-1");
    });

    it("arrow navigation rolls the tab stop onto the newly-focused item", async () => {
      const anchorEl = createAnchor();
      const { getByText } = render(Harness, {
        anchorEl,
        initialOpen: true,
        items: [{ label: "A" }, { label: "B" }, { label: "C" }],
      });
      flushSync();
      expect(getByText("A").getAttribute("tabindex")).toBe("0");

      await fireEvent.keyDown(document.activeElement as Element, { key: "ArrowDown" });
      flushSync();
      expect(getByText("A").getAttribute("tabindex")).toBe("-1");
      expect(getByText("B").getAttribute("tabindex")).toBe("0");
      expect(getByText("C").getAttribute("tabindex")).toBe("-1");

      await fireEvent.keyDown(document.activeElement as Element, { key: "End" });
      flushSync();
      expect(getByText("B").getAttribute("tabindex")).toBe("-1");
      expect(getByText("C").getAttribute("tabindex")).toBe("0");
    });
  });
});
