import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { render, within } from "@testing-library/svelte";
import Harness from "./__fixtures__/LayoutHarness.svelte";
import { MockResizeObserver } from "$lib/ui/hooks/__fixtures__/resize-observer-mock";
import { Navigation } from "../state/navigation.svelte";
import { Ui } from "../state/ui.svelte";
import type { ApiClient } from "../api/client";

beforeEach(() => {
  vi.stubGlobal("ResizeObserver", MockResizeObserver);
});

afterEach(() => {
  vi.unstubAllGlobals();
});

function failedNavigation(): Navigation {
  const navigation = new Navigation({} as ApiClient);
  navigation.loading = false;
  navigation.error = "boom";
  return navigation;
}

describe("Layout navigation error", () => {
  it("shows one alert in the desktop sidebar", () => {
    const { getByRole } = render(Harness, { navigation: failedNavigation() });
    const alerts = within(getByRole("complementary", { name: "Sidebar" })).getAllByRole("alert");
    expect(alerts).toHaveLength(1);
    expect(alerts[0].textContent?.trim()).toBe("Failed to load navigation: boom");
  });

  it("shows one alert in the mobile drawer", () => {
    const ui = new Ui();
    ui.openMobileMenu();
    const { getByRole } = render(Harness, { navigation: failedNavigation(), ui });
    const drawer = getByRole("dialog", { name: "Mobile navigation" });
    const alerts = within(drawer).getAllByRole("alert");
    expect(alerts).toHaveLength(1);
    expect(alerts[0].textContent?.trim()).toBe("Failed to load navigation: boom");
  });
});
