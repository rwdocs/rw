import { describe, it, expect, beforeEach } from "vitest";
import { get } from "svelte/store";
import { mobileMenuOpen, openMobileMenu, closeMobileMenu } from "./ui";

describe("mobileMenuOpen store", () => {
  beforeEach(() => {
    // Reset to closed state
    closeMobileMenu();
  });

  it("starts closed", () => {
    expect(get(mobileMenuOpen)).toBe(false);
  });

  it("opens with openMobileMenu", () => {
    openMobileMenu();

    expect(get(mobileMenuOpen)).toBe(true);
  });

  it("closes with closeMobileMenu", () => {
    openMobileMenu();
    closeMobileMenu();

    expect(get(mobileMenuOpen)).toBe(false);
  });

  it("can be toggled multiple times", () => {
    expect(get(mobileMenuOpen)).toBe(false);

    openMobileMenu();
    expect(get(mobileMenuOpen)).toBe(true);

    closeMobileMenu();
    expect(get(mobileMenuOpen)).toBe(false);

    openMobileMenu();
    expect(get(mobileMenuOpen)).toBe(true);
  });
});
