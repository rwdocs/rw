import { readFile, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { test, expect } from "@playwright/test";

/** The live-reload fixture, rewritten (and restored) by the reload test below. */
const FIXTURE = fileURLToPath(new URL("./fixtures/docs/diagram-live.md", import.meta.url));

/**
 * The live-reload test drives a *second* `rw serve` that has live reload on
 * (see `playwright.config.ts`). Rewriting a fixture on the shared server would
 * push a reload into every other spec's page, and the suite is fullyParallel.
 */
const LIVE_URL = "http://127.0.0.1:8084/diagram-live";

test.describe("Diagram id isolation", () => {
  test.use({ viewport: { width: 1200, height: 800 } });

  test("each diagram resolves its own clip path", async ({ page }) => {
    await page.goto("/diagram-collision");

    const figures = page.locator("figure.diagram");
    await expect(figures).toHaveCount(2);

    // Both SVGs must be inside a shadow root, not the light tree.
    expect(await page.locator("figure.diagram > rw-diagram").count()).toBe(2);
    expect(await page.locator("figure.diagram > svg").count()).toBe(0);

    // The id that collides exists twice — once per root — and never in the document.
    const perRoot = await page.evaluate(() =>
      [...document.querySelectorAll("rw-diagram")].map(
        (el) => el.shadowRoot?.getElementById("clip1")?.tagName ?? null,
      ),
    );
    expect(perRoot).toEqual(["clipPath", "clipPath"]);
    expect(await page.evaluate(() => document.getElementById("clip1") === null)).toBe(true);

    // Each rect's clip-path must resolve to the clipPath in its OWN root. If ids
    // leaked, both would resolve to the first diagram's full-size clipPath.
    const clipWidths = await page.evaluate(() =>
      [...document.querySelectorAll("rw-diagram")].map((el) => {
        const rect = el.shadowRoot?.querySelector("clipPath rect");
        return rect?.getAttribute("width") ?? null;
      }),
    );
    // First diagram clips to full size; second clips to nothing.
    expect(clipWidths).toEqual(["200", "0"]);

    // Painted proof, not just structure: hit-testing honours clip-path. The first
    // rect is clipped to full size and is hit; the second is clipped to nothing
    // and is not. Document-wide id resolution would make the second rect resolve
    // the first diagram's clipPath and become hit-testable too.
    const hits = await page.evaluate(() =>
      [...document.querySelectorAll("rw-diagram")].map((el) => {
        const root = el.shadowRoot!;
        const rect = root.querySelector<SVGRectElement>("svg > rect")!;
        const b = rect.getBoundingClientRect();
        const hit = root.elementFromPoint(b.x + b.width / 2, b.y + b.height / 2);
        return hit?.getAttribute("data-testid") ?? hit?.tagName ?? null;
      }),
    );
    expect(hits[0]).toBe("first-rect");
    expect(hits[1]).not.toBe("second-rect");
    // Positive control: prove the second diagram painted at all, rather than
    // this passing because it silently failed to render. The probe lands on
    // the second diagram's own <svg> root (the rect is genuinely clipped away,
    // so hit-testing falls through to the svg behind it) — not null.
    expect(hits[1]).toBe("svg");
  });

  test("a wrapped diagram is styled by the sheet its shadow root adopts", async ({ page }) => {
    // The shadow boundary that isolates ids also cuts the diagram off from
    // `content.css`, so its sizing and font now arrive at runtime via
    // `applySheet`. Nothing else in the suite exercises that styling — only the
    // structure around it — and jsdom cannot: it has no `adoptedStyleSheets`,
    // so the unit tests only ever reach the `<style>` fallback.
    await page.goto("/diagram-style");

    const wrapped = page.locator('figure[data-diagram-id="styled-wrapped"]');
    const bare = page.locator('figure[data-diagram-id="styled-bare"]');
    await expect(wrapped.locator("rw-diagram")).toHaveCount(1);
    // The bare figure is the control: it keeps its SVG in the light tree and is
    // styled by `content.css`, so it shows what the wrapped one must match.
    await expect(bare.locator("> svg")).toHaveCount(1);

    // The adopted path, not the fallback: a real browser must take the
    // constructable-stylesheet branch, and share one sheet object across roots.
    const adoption = await page.evaluate(() => {
      const roots = [...document.querySelectorAll("rw-diagram")].map((el) => el.shadowRoot!);
      return {
        sheetCounts: roots.map((r) => r.adoptedStyleSheets.length),
        styleFallbacks: roots.map((r) => r.querySelectorAll("style").length),
      };
    });
    expect(adoption.sheetCounts).toEqual([1]);
    expect(adoption.styleFallbacks).toEqual([0]);

    // `max-width: 100%` from the adopted sheet: the SVG's intrinsic width is
    // 1200px, and it must render no wider than the column that holds it.
    const widths = await page.evaluate(() => {
      const svgOf = (id: string) => {
        const figure = document.querySelector<HTMLElement>(`figure[data-diagram-id="${id}"]`)!;
        const host = figure.querySelector("rw-diagram");
        const svg = (host?.shadowRoot ?? figure).querySelector("svg")!;
        return {
          svg: svg.getBoundingClientRect().width,
          figure: figure.clientWidth,
          figureHeight: figure.getBoundingClientRect().height,
          svgHeight: svg.getBoundingClientRect().height,
        };
      };
      const article = document.querySelector("article")!;
      return {
        wrapped: svgOf("styled-wrapped"),
        bare: svgOf("styled-bare"),
        overflow: article.scrollWidth - article.clientWidth,
      };
    });
    // The column really is narrower than the SVG, so the rule had work to do.
    expect(widths.wrapped.figure).toBeLessThan(1200);
    expect(widths.wrapped.svg).toBeLessThanOrEqual(widths.wrapped.figure);
    // Wrapped and bare are sized identically — the adopted sheet reproduces
    // exactly what the light-DOM rule gives the un-wrapped figure.
    expect(Math.round(widths.wrapped.svg)).toBe(Math.round(widths.bare.svg));

    // Height parity, which is what `display: block` in the adopted sheet buys.
    // An un-wrapped SVG is a direct child of the flex figure and so is
    // blockified; inside the wrapper it would be inline-level, building a line
    // box that adds ~9px of descender space under every diagram. Without the
    // rule the wrapped figure is taller than the bare one by exactly that gap.
    expect(Math.round(widths.wrapped.figureHeight)).toBe(Math.round(widths.bare.figureHeight));
    expect(Math.round(widths.wrapped.figureHeight)).toBe(Math.round(widths.wrapped.svgHeight));

    // And nothing spills sideways out of the article.
    expect(widths.overflow).toBeLessThanOrEqual(0);

    // `svg text { font-family: Roboto }` and `svg a { text-decoration: none }`
    // from the same sheet. Computed style is read inside the shadow root, which
    // is where the rules live.
    const text = await page.evaluate(() => {
      const root = document.querySelector("rw-diagram")!.shadowRoot!;
      const label = root.querySelector('[data-testid="wrapped-text"]')!;
      const link = root.querySelector('[data-testid="wrapped-link"]')!;
      const bareLabel = document.querySelector('[data-testid="bare-text"]')!;
      return {
        family: getComputedStyle(label).fontFamily,
        bareFamily: getComputedStyle(bareLabel).fontFamily,
        decoration: getComputedStyle(link).textDecorationLine,
      };
    });
    expect(text.family).toContain("Roboto");
    expect(text.family).toBe(text.bareFamily);
    expect(text.decoration).toBe("none");

    // Dark mode inverts the wrapped diagram. This rule deliberately does NOT
    // live in the adopted sheet — no selector inside a shadow root can match
    // `.dark` on the document root — so it is applied to the light-DOM host
    // instead, and `filter` carries into the shadow subtree as a paint effect.
    await page.emulateMedia({ colorScheme: "dark" });
    await expect(page.locator("html")).toHaveClass(/dark/);
    const filters = await page.evaluate(() => ({
      host: getComputedStyle(document.querySelector("rw-diagram")!).filter,
      bare: getComputedStyle(document.querySelector('figure[data-diagram-id="styled-bare"] > svg')!)
        .filter,
      // The expand button's injected icon must stay un-inverted (content.css
      // overrides it with `filter: none`), or it renders as its own negative.
      icon: getComputedStyle(
        document.querySelector('figure[data-diagram-id="styled-wrapped"] .diagram-expand-btn svg')!,
      ).filter,
    }));
    expect(filters.host).toContain("invert");
    expect(filters.bare).toContain("invert");
    expect(filters.icon).toBe("none");
  });

  test("live reload with the zoom popup open keeps it open and shows no error", async ({
    page,
  }) => {
    const original = await readFile(FIXTURE, "utf8");
    try {
      await page.goto(LIVE_URL);
      // The expand button is hover-revealed (opacity), so force the click.
      await page.getByRole("button", { name: "Expand diagram" }).click({ force: true });
      const dialog = page.getByRole("dialog", { name: "Diagram viewer" });
      await expect(dialog).toBeVisible();

      // Rewrite the diagram's source — rw serve watches the fixture dir and
      // pushes a live reload over the websocket.
      await writeFile(FIXTURE, original.replace('fill="teal"', 'fill="purple"'), "utf8");

      // Wait on the observable effect of the reload rather than a bare timeout.
      // (`page.locator` pierces open shadow roots, so scope this to the article —
      // the popup's clone carries the same test id.)
      const articleRect = page
        .getByRole("article")
        .locator('[data-testid="live-rect"][fill="purple"]');
      await expect(articleRect).toHaveCount(1);

      // The popup re-resolved to the new render rather than keeping a stale clone.
      await expect(
        page
          .getByTestId("diagram-zoom-content")
          .locator('[data-testid="live-rect"][fill="purple"]'),
      ).toHaveCount(1);

      // The popup must survive the reload, keep showing the diagram, and must NOT
      // report a healthy diagram as broken.
      await expect(dialog).toBeVisible();
      await expect(page.getByTestId("diagram-zoom-content").locator("svg")).toBeVisible();
      await expect(page.getByText("check its source for errors")).toHaveCount(0);
      await expect(page.locator('figure[data-diagram-id="live"] > rw-diagram')).toHaveCount(1);
    } finally {
      await writeFile(FIXTURE, original, "utf8");
    }
  });

  test("popup clone keeps its own id scope (scoped style and url(#…) reference)", async ({
    page,
  }) => {
    // Its own fixture, on the shared server: the live-reload test rewrites
    // `diagram-live.md` in place and the suite is fullyParallel, so reading that
    // file here would race with it.
    await page.goto("/diagram-scope");
    await page.getByRole("button", { name: "Expand diagram" }).click({ force: true });
    await expect(page.getByRole("dialog", { name: "Diagram viewer" })).toBeVisible();

    // The fixture styles this rect via a rule scoped to the SVG root id. The
    // clone mounts into the modal's own shadow root, so its ids are no longer
    // rewritten — the scoped rule must still match, or the rect falls back to
    // SVG's default black fill.
    const styled = page.getByTestId("diagram-zoom-content").locator('[data-testid="scope-styled"]');
    await expect(styled).toBeVisible();
    expect(await styled.evaluate((el) => getComputedStyle(el).fill)).toBe("rgb(238, 238, 255)");

    // The same rect (user units x 100..200) is clipped by `url(#scope-clip)` to
    // its left half (x 100..150). Hit-testing honours clip-path, so the right
    // half must NOT hit the rect — that only holds if the fragment reference
    // resolved inside the modal's shadow root. Chromium renders an element whose
    // clip-path reference does not resolve *unclipped*, so a broken reference
    // makes the right-half probe hit the rect and this test fail.
    const hits = await page.evaluate(() => {
      const root = document.querySelector<HTMLElement>(
        '[data-testid="diagram-zoom-content"]',
      )!.shadowRoot!;
      const svg = root.querySelector("svg")!;
      const m = svg.getScreenCTM()!;
      const probe = (x: number, y: number) => {
        const el = root.elementFromPoint(m.a * x + m.c * y + m.e, m.b * x + m.d * y + m.f);
        return el?.getAttribute("data-testid") ?? el?.tagName ?? null;
      };
      return { inside: probe(125, 50), clipped: probe(175, 50) };
    });
    expect(hits.inside).toBe("scope-styled");
    expect(hits.clipped).not.toBe("scope-styled");
    // Positive control: prove the clip actually removed the rect from hit-testing
    // rather than this passing because nothing rendered there. The probe still
    // lands on the clone's own <svg> root, not null.
    //
    // Honest caveat: the clone is a byte-identical copy of the original (same
    // ids, same geometry), so no assertion here can tell "the reference resolved
    // inside the modal's shadow root" apart from "it resolved to the article's
    // identical copy instead". What this test actually proves is the property
    // that made deleting `namespaceIds.ts` safe — the clone paints correctly
    // with un-rewritten ids — not the resolution mechanism.
    expect(hits.clipped).toBe("svg");
  });
});
