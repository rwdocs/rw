import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { trapFocus } from "./trapFocus";

describe("trapFocus", () => {
  let container: HTMLDivElement;
  let outside: HTMLButtonElement;
  let first: HTMLButtonElement;
  let last: HTMLButtonElement;

  beforeEach(() => {
    document.body.innerHTML = "";

    outside = document.createElement("button");
    outside.textContent = "outside";
    document.body.appendChild(outside);

    container = document.createElement("div");
    container.tabIndex = -1;
    first = document.createElement("button");
    first.textContent = "first";
    last = document.createElement("button");
    last.textContent = "last";
    container.append(first, last);
    document.body.appendChild(container);

    outside.focus();
  });

  afterEach(() => {
    document.body.innerHTML = "";
  });

  function tab(target: HTMLElement, shiftKey = false): KeyboardEvent {
    const event = new KeyboardEvent("keydown", {
      key: "Tab",
      shiftKey,
      bubbles: true,
      cancelable: true,
    });
    target.dispatchEvent(event);
    return event;
  }

  it("moves focus to the container on attach", () => {
    const cleanup = trapFocus(container);
    expect(document.activeElement).toBe(container);
    cleanup();
  });

  it("wraps Tab from the last focusable to the first", () => {
    const cleanup = trapFocus(container);
    last.focus();
    const event = tab(last);
    expect(event.defaultPrevented).toBe(true);
    expect(document.activeElement).toBe(first);
    cleanup();
  });

  it("wraps Shift+Tab from the first focusable to the last", () => {
    const cleanup = trapFocus(container);
    first.focus();
    const event = tab(first, true);
    expect(event.defaultPrevented).toBe(true);
    expect(document.activeElement).toBe(last);
    cleanup();
  });

  it("does not intercept Tab in the middle of the list", () => {
    const middle = document.createElement("button");
    container.insertBefore(middle, last);
    const cleanup = trapFocus(container);
    middle.focus();
    const event = tab(middle);
    // Browser default is left alone (jsdom does not move focus on Tab, so the
    // active element stays put — the point is the trap did not force a wrap).
    expect(event.defaultPrevented).toBe(false);
    expect(document.activeElement).toBe(middle);
    cleanup();
  });

  it("keeps Tab and Shift+Tab on the sole focusable element", () => {
    container.replaceChildren();
    const only = document.createElement("button");
    only.textContent = "only";
    container.appendChild(only);
    const cleanup = trapFocus(container);
    only.focus();

    const forward = tab(only);
    expect(forward.defaultPrevented).toBe(true);
    expect(document.activeElement).toBe(only);

    const backward = tab(only, true);
    expect(backward.defaultPrevented).toBe(true);
    expect(document.activeElement).toBe(only);
    cleanup();
  });

  it("pins focus to the host when there is nothing focusable inside", () => {
    container.replaceChildren();
    const cleanup = trapFocus(container);
    expect(document.activeElement).toBe(container);
    const event = tab(container);
    expect(event.defaultPrevented).toBe(true);
    expect(document.activeElement).toBe(container);
    cleanup();
  });

  it("enters the first focusable on the initial Tab from the host", () => {
    const cleanup = trapFocus(container);
    // After attach, focus is on the container itself.
    expect(document.activeElement).toBe(container);
    const event = tab(container);
    expect(event.defaultPrevented).toBe(true);
    expect(document.activeElement).toBe(first);
    cleanup();
  });

  it("enters the last focusable on the initial Shift+Tab from the host", () => {
    const cleanup = trapFocus(container);
    expect(document.activeElement).toBe(container);
    const event = tab(container, true);
    expect(event.defaultPrevented).toBe(true);
    expect(document.activeElement).toBe(last);
    cleanup();
  });

  it("restores focus to the previously focused element on cleanup", () => {
    const cleanup = trapFocus(container);
    expect(document.activeElement).toBe(container);
    cleanup();
    expect(document.activeElement).toBe(outside);
  });

  it("skips focus restore when the previously focused element is gone", () => {
    const cleanup = trapFocus(container);
    expect(document.activeElement).toBe(container);
    // The element that had focus at open time is removed before close.
    outside.remove();
    cleanup();
    // No throw, and focus is not forced back onto the detached element.
    expect(document.activeElement).not.toBe(outside);
  });
});
