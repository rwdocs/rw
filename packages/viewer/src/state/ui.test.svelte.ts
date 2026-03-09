import { describe, it, expect } from "vitest";
import { Ui } from "./ui.svelte";

describe("ui store", () => {
  it("starts with menu closed", () => {
    const ui = new Ui();
    expect(ui.mobileMenuOpen).toBe(false);
  });

  it("opens with openMobileMenu", () => {
    const ui = new Ui();
    ui.openMobileMenu();

    expect(ui.mobileMenuOpen).toBe(true);
  });

  it("closes with closeMobileMenu", () => {
    const ui = new Ui();
    ui.openMobileMenu();
    ui.closeMobileMenu();

    expect(ui.mobileMenuOpen).toBe(false);
  });

  it("can be toggled multiple times", () => {
    const ui = new Ui();
    expect(ui.mobileMenuOpen).toBe(false);

    ui.openMobileMenu();
    expect(ui.mobileMenuOpen).toBe(true);

    ui.closeMobileMenu();
    expect(ui.mobileMenuOpen).toBe(false);

    ui.openMobileMenu();
    expect(ui.mobileMenuOpen).toBe(true);
  });

  it("each instance is independent", () => {
    const ui1 = new Ui();
    const ui2 = new Ui();

    ui1.openMobileMenu();

    expect(ui1.mobileMenuOpen).toBe(true);
    expect(ui2.mobileMenuOpen).toBe(false);
  });
});
