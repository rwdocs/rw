import { readFile, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { test, expect } from "@playwright/test";

// No `?` page: Git for Windows refuses to check out such a name, and CI tests on Windows.

/** Rewritten (and restored) by the live-reload test, so no other test reads it. */
const LIVE_FIXTURE = fileURLToPath(new URL("./fixtures/docs/symbols/edit%23.md", import.meta.url));

/** The live-reload server; see `rw-live.toml`. */
const LIVE_URL = "http://127.0.0.1:8084/symbols";

test.describe("Page names with URL delimiters", () => {
  for (const { name, text } of [
    { name: "C#", text: "The C# page." },
    { name: "100%25", text: "The 100%25 page." },
  ]) {
    test(`clicking the ${name} nav item opens it and marks it current`, async ({ page }) => {
      await page.goto("/symbols");
      const aside = page.getByRole("complementary", { name: "Sidebar" });

      await aside.getByRole("link", { name, exact: true }).click();

      await expect(page.getByRole("article")).toContainText(text);
      await expect(aside.getByRole("link", { name, exact: true })).toHaveAttribute(
        "aria-current",
        "page",
      );
    });
  }

  test("opening a page by URL marks its nav item and expands its section", async ({ page }) => {
    await page.goto("/symbols/c%23");

    await expect(page.getByRole("article")).toContainText("The C# page.");
    const aside = page.getByRole("complementary", { name: "Sidebar" });
    await expect(aside.getByRole("link", { name: "C#", exact: true })).toHaveAttribute(
      "aria-current",
      "page",
    );
  });

  test("a wikilink into a section folder named sys#1 opens its page", async ({ page }) => {
    await page.goto("/reserved/c%23/intro");

    await page.getByRole("article").getByRole("link", { name: "the sys1 reference" }).click();

    await expect(page).toHaveURL(/\/reserved\/sys%231\/api$/);
    await expect(page.getByRole("heading", { level: 1 })).toContainText("Sys API");
  });

  test("live reload refreshes a page reached by a nav click", async ({ page }) => {
    const original = await readFile(LIVE_FIXTURE, "utf8");
    try {
      await page.goto(LIVE_URL);
      await page
        .getByRole("complementary", { name: "Sidebar" })
        .getByRole("link", { name: "Edit#" })
        .click();
      await expect(page.getByRole("article")).toContainText("Original text.");

      await writeFile(LIVE_FIXTURE, original.replace("Original", "Edited"), "utf8");

      await expect(page.getByRole("article")).toContainText("Edited text.");
    } finally {
      await writeFile(LIVE_FIXTURE, original, "utf8");
    }
  });
});
