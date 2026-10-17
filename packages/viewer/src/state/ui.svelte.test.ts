import { describe, it, expect } from "vitest";
import { Ui } from "./ui.svelte";

describe("Ui toast queue", () => {
  it("pushToast enqueues a toast", () => {
    const ui = new Ui();
    ui.pushToast({ intent: "error", message: "boom" });
    expect(ui.toasts).toHaveLength(1);
    expect(ui.toasts[0]).toMatchObject({ intent: "error", message: "boom" });
  });

  it("de-dupes an identical (intent + message) toast", () => {
    const ui = new Ui();
    ui.pushToast({ intent: "error", message: "boom" });
    ui.pushToast({ intent: "error", message: "boom" });
    expect(ui.toasts).toHaveLength(1);
  });

  it("keeps toasts that differ in intent or message", () => {
    const ui = new Ui();
    ui.pushToast({ intent: "error", message: "boom" });
    ui.pushToast({ intent: "error", message: "other" });
    ui.pushToast({ intent: "info", message: "boom" });
    expect(ui.toasts).toHaveLength(3);
  });

  it("dismissToast removes the matching toast by id", () => {
    const ui = new Ui();
    ui.pushToast({ intent: "error", message: "boom" });
    ui.pushToast({ intent: "info", message: "hi" });
    ui.dismissToast(ui.toasts[0].id);
    expect(ui.toasts.map((t) => t.message)).toEqual(["hi"]);
  });
});

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
