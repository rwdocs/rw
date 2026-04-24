import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render } from "@testing-library/svelte";
import { tick } from "svelte";
import LoadingBar from "./LoadingBar.svelte";

describe("LoadingBar", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("stays idle while loading is false", () => {
    const { queryByRole } = render(LoadingBar, { loading: false });
    expect(queryByRole("progressbar")).toBeNull();
  });

  it("defaults threshold to 300ms and renders a labeled, busy progressbar after it elapses", async () => {
    const { queryByRole } = render(LoadingBar, { loading: true });
    await vi.advanceTimersByTimeAsync(299);
    await tick();
    expect(queryByRole("progressbar")).toBeNull();

    await vi.advanceTimersByTimeAsync(1);
    await tick();
    const bar = queryByRole("progressbar", { name: "Page loading" });
    expect(bar).not.toBeNull();
    expect(bar?.getAttribute("aria-busy")).toBe("true");
  });

  it("honors a custom threshold prop", async () => {
    const { queryByRole } = render(LoadingBar, { loading: true, threshold: 50 });
    await vi.advanceTimersByTimeAsync(49);
    await tick();
    expect(queryByRole("progressbar")).toBeNull();

    await vi.advanceTimersByTimeAsync(1);
    await tick();
    expect(queryByRole("progressbar")).not.toBeNull();
  });
});
