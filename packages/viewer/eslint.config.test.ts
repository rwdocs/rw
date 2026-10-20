// @vitest-environment node
import { ESLint } from "eslint";
import { resolve } from "node:path";
import tseslint from "typescript-eslint";
import { beforeAll, describe, expect, it } from "vitest";

// The Tailwind rules need no type information, and starting the TypeScript
// project service for the type-aware ones is slow enough to time out on CI.
const eslint = new ESLint({
  cwd: import.meta.dirname,
  overrideConfigFile: resolve(import.meta.dirname, "eslint.config.js"),
  overrideConfig: tseslint.configs.disableTypeChecked,
});

// Existing paths exercise the real config scopes without writing the
// deliberately invalid class names into the source tree.
const kitPath = resolve(import.meta.dirname, "src/lib/ui/primitives/Button.svelte");
const componentPath = resolve(import.meta.dirname, "src/components/Breadcrumbs.svelte");

function component(classes: string, table = "VARIANT_CLASSES", markup = "inline-flex") {
  return `<script module lang="ts">
  const ${table} = { primary: ${JSON.stringify(classes)} };
  const LABELS = { primary: "Not a Tailwind utility" };
</script>
<button type="button" class={[${table}.primary, ${JSON.stringify(markup)}]}>{LABELS.primary}</button>`;
}

async function diagnostics(source: string, filePath = kitPath) {
  const results = await eslint.lintText(source, { filePath });
  expect(results).toHaveLength(1);
  return results[0].messages.map(({ ruleId, severity }) => ({ ruleId, severity }));
}

describe("Tailwind class-table lint coverage", () => {
  // The first lint starts the Tailwind worker.
  beforeAll(async () => {
    await eslint.lintText(component("bg-bg-raised"), { filePath: kitPath });
  }, 30_000);

  it.each(["VARIANT_CLASSES", "SIZE_CLASSES", "INTENT_CLASSES", "ROTATE_CLASSES"])(
    "rejects unknown utilities in %s",
    async (table) => {
      await expect(diagnostics(component("rw-lint-unknown", table))).resolves.toEqual([
        { ruleId: "better-tailwindcss/no-unknown-classes", severity: 2 },
      ]);
    },
  );

  it.each(["bg-red-500", "bg-accent-500"])(
    "rejects primitive color %s in kit class tables",
    async (classes) => {
      await expect(diagnostics(component(classes))).resolves.toEqual([
        { ruleId: "better-tailwindcss/no-restricted-classes", severity: 2 },
      ]);
    },
  );

  it.each(["variant_CLASSES", "VARIANT_CLASSES_EXTRA"])(
    "does not broaden table matching to %s",
    async (table) => {
      await expect(diagnostics(component("rw-lint-unknown", table))).resolves.toEqual([]);
    },
  );

  it("accepts semantic tokens and ordinary text without imposing class order on tables", async () => {
    await expect(diagnostics(component("text-fg-default bg-bg-raised"))).resolves.toEqual([]);
  });

  it("still checks class attributes when adding table selectors", async () => {
    await expect(
      diagnostics(component("bg-bg-raised", "VARIANT_CLASSES", "rw-lint-unknown")),
    ).resolves.toEqual([{ ruleId: "better-tailwindcss/no-unknown-classes", severity: 2 }]);
  });

  it("still restricts primitive colors in kit class attributes", async () => {
    await expect(
      diagnostics(component("bg-bg-raised", "VARIANT_CLASSES", "bg-red-500")),
    ).resolves.toEqual([{ ruleId: "better-tailwindcss/no-restricted-classes", severity: 2 }]);
  });

  it("preserves the custom layout-class exception", async () => {
    await expect(
      diagnostics(component("layout-probe", "VARIANT_CLASSES", "layout-probe")),
    ).resolves.toEqual([]);
  });

  it("checks tables outside the kit without extending its color policy", async () => {
    await expect(diagnostics(component("bg-red-500"), componentPath)).resolves.toEqual([]);
    await expect(diagnostics(component("rw-lint-unknown"), componentPath)).resolves.toEqual([
      { ruleId: "better-tailwindcss/no-unknown-classes", severity: 2 },
    ]);
  });
});
