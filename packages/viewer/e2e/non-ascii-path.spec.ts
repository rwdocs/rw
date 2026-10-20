import { readFile, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { test, expect } from "@playwright/test";

/** Rewritten (and restored) by the live-reload test, so no other test reads it. */
const LIVE_FIXTURE = fileURLToPath(new URL("./fixtures/docs/гайд/правка.md", import.meta.url));

/** The live-reload server; see `rw-live.toml`. */
const LIVE_URL = "http://127.0.0.1:8084/гайд/правка";

test.describe("Non-ASCII page paths", () => {
  test("opening a page by URL marks its nav item and expands its section", async ({ page }) => {
    await page.goto("/гайд/шаг");

    await expect(page.getByRole("article")).toContainText("Первый шаг.");
    const aside = page.getByRole("complementary", { name: "Sidebar" });
    await expect(aside.getByRole("link", { name: "Шаг" })).toBeVisible();
    await expect(aside.getByRole("link", { name: "Шаг" })).toHaveAttribute("aria-current", "page");
  });

  test("clicking a nav item marks it current", async ({ page }) => {
    await page.goto("/гайд");
    const aside = page.getByRole("complementary", { name: "Sidebar" });

    await aside.getByRole("link", { name: "Шаг" }).click();

    await expect(page.getByRole("article")).toContainText("Первый шаг.");
    await expect(aside.getByRole("link", { name: "Шаг" })).toHaveAttribute("aria-current", "page");
  });

  test("clicking a content link opens the page", async ({ page }) => {
    await page.goto("/гайд");

    await page.getByRole("article").getByRole("link", { name: "К шагу" }).click();

    await expect(page.getByRole("article")).toContainText("Первый шаг.");
    await expect(page).toHaveURL(/\/%D0%B3%D0%B0%D0%B9%D0%B4\/%D1%88%D0%B0%D0%B3$/);
  });

  test("live reload refreshes the page on screen", async ({ page }) => {
    const original = await readFile(LIVE_FIXTURE, "utf8");
    try {
      await page.goto(LIVE_URL);
      await expect(page.getByRole("article")).toContainText("Исходный текст.");

      await writeFile(LIVE_FIXTURE, original.replace("Исходный", "Новый"), "utf8");

      await expect(page.getByRole("article")).toContainText("Новый текст.");
    } finally {
      await writeFile(LIVE_FIXTURE, original, "utf8");
    }
  });
});
