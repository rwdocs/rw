import { describe, it, expect, vi } from "vitest";
import { Ui } from "./ui.svelte";
import { createNotify } from "../lib/notify";

describe("Ui toast queue", () => {
  it("pushToast enqueues a toast and returns its id", () => {
    const ui = new Ui();
    const id = ui.pushToast({ intent: "error", message: "boom" });
    expect(ui.toasts).toHaveLength(1);
    expect(ui.toasts[0]).toMatchObject({ id, intent: "error", message: "boom" });
  });

  it("de-dupes an identical (intent + message) toast", () => {
    const ui = new Ui();
    const first = ui.pushToast({ intent: "error", message: "boom" });
    const second = ui.pushToast({ intent: "error", message: "boom" });
    expect(ui.toasts).toHaveLength(1);
    expect(second).toBe(first);
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
    const id = ui.pushToast({ intent: "error", message: "boom" });
    ui.pushToast({ intent: "info", message: "hi" });
    ui.dismissToast(id);
    expect(ui.toasts.map((t) => t.message)).toEqual(["hi"]);
  });
});

describe("createNotify", () => {
  it("returns the host callback when provided and leaves the Ui queue empty", () => {
    const ui = new Ui();
    const onNotify = vi.fn();
    const notify = createNotify(ui, onNotify);
    notify({ intent: "error", message: "boom" });
    expect(onNotify).toHaveBeenCalledWith({ intent: "error", message: "boom" });
    expect(ui.toasts).toHaveLength(0);
  });

  it("falls back to the Ui toaster when no host callback is given", () => {
    const ui = new Ui();
    const notify = createNotify(ui);
    notify({ intent: "error", message: "boom" });
    expect(ui.toasts).toHaveLength(1);
    expect(ui.toasts[0].message).toBe("boom");
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
