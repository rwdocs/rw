import { describe, it, expect } from "vitest";
import { get } from "svelte/store";
import { createUiStore } from "./ui";

describe("ui store", () => {
  it("starts with menu closed", () => {
    const ui = createUiStore();
    expect(get(ui).mobileMenuOpen).toBe(false);
  });

  it("opens with openMobileMenu", () => {
    const ui = createUiStore();
    ui.openMobileMenu();

    expect(get(ui).mobileMenuOpen).toBe(true);
  });

  it("closes with closeMobileMenu", () => {
    const ui = createUiStore();
    ui.openMobileMenu();
    ui.closeMobileMenu();

    expect(get(ui).mobileMenuOpen).toBe(false);
  });

  it("can be toggled multiple times", () => {
    const ui = createUiStore();
    expect(get(ui).mobileMenuOpen).toBe(false);

    ui.openMobileMenu();
    expect(get(ui).mobileMenuOpen).toBe(true);

    ui.closeMobileMenu();
    expect(get(ui).mobileMenuOpen).toBe(false);

    ui.openMobileMenu();
    expect(get(ui).mobileMenuOpen).toBe(true);
  });

  it("each instance is independent", () => {
    const ui1 = createUiStore();
    const ui2 = createUiStore();

    ui1.openMobileMenu();

    expect(get(ui1).mobileMenuOpen).toBe(true);
    expect(get(ui2).mobileMenuOpen).toBe(false);
  });

  it("starts with tocPopoverOpen false", () => {
    const ui = createUiStore();
    expect(get(ui).tocPopoverOpen).toBe(false);
  });

  it("toggleTocPopover toggles tocPopoverOpen", () => {
    const ui = createUiStore();
    ui.toggleTocPopover();
    expect(get(ui).tocPopoverOpen).toBe(true);

    ui.toggleTocPopover();
    expect(get(ui).tocPopoverOpen).toBe(false);
  });

  it("closeTocPopover closes tocPopoverOpen", () => {
    const ui = createUiStore();
    ui.toggleTocPopover();
    expect(get(ui).tocPopoverOpen).toBe(true);

    ui.closeTocPopover();
    expect(get(ui).tocPopoverOpen).toBe(false);
  });
});
