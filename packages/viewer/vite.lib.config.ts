import { defineConfig, type Plugin } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import tailwindcss from "@tailwindcss/vite";
import dts from "vite-plugin-dts";
import { readFile, writeFile } from "fs/promises";
import { resolve } from "path";
import { fileURLToPath } from "url";
import { transform, type SelectorComponent, type Selector } from "lightningcss";
import { licenseNotice } from "./license-notice";

const __dirname = fileURLToPath(new URL(".", import.meta.url));

const SCOPE: SelectorComponent = { type: "attribute", name: "data-rw-viewer" };
const DESCENDANT: SelectorComponent = { type: "combinator", value: "descendant" };

function isGlobal(component: SelectorComponent): boolean {
  if (component.type === "type" && (component.name === "html" || component.name === "body"))
    return true;
  if (component.type === "pseudo-class" && (component.kind === "root" || component.kind === "host"))
    return true;
  if (component.type === "id" && component.name === "app") return true;
  return false;
}

function hasScope(selector: Selector): boolean {
  return selector.some((c) => c.type === "attribute" && c.name === "data-rw-viewer");
}

/** `mountRw` toggles `.dark` on the scope element itself, so a selector led by
 *  `.dark` must also match as a compound with the scope, not only below it. */
function startsWithRootState(selector: Selector): boolean {
  const end = selector.findIndex((c) => c.type === "combinator");
  const head = end === -1 ? selector : selector.slice(0, end);
  return head.some((c) => c.type === "class" && c.name === "dark");
}

/**
 * Strip `@layer <name> { ... }` wrappers, keeping inner rules.
 * Un-layered CSS beats layered CSS regardless of specificity, which is
 * needed so the viewer's scoped rules beat host resets like MUI's CssBaseline.
 *
 * This is done as string operations instead of lightningcss's Rule visitor because:
 * - The Rule visitor has a serialization bug with var() values that breaks on
 *   Tailwind's CSS output (https://github.com/parcel-bundler/lightningcss/issues/1081)
 * - @layer is a built-in at-rule that cannot be intercepted via customAtRules
 *   (https://github.com/parcel-bundler/lightningcss/discussions/945)
 */
function stripLayers(css: string): string {
  // NOTE: Brace tracking does not handle braces inside CSS string literals
  // (e.g., content: "{") or comments. This is fine because the input is
  // Tailwind-generated CSS which does not contain such patterns.
  const out: string[] = [];
  let i = 0;
  let runStart = 0;
  const len = css.length;

  while (i < len) {
    if (css.startsWith("@layer ", i)) {
      if (runStart < i) out.push(css.slice(runStart, i));

      const braceIdx = css.indexOf("{", i);
      if (braceIdx === -1) break;

      // @layer declaration (ordering) like `@layer a, b;` — strip entirely
      const semiIdx = css.indexOf(";", i);
      if (semiIdx !== -1 && semiIdx < braceIdx) {
        i = semiIdx + 1;
        while (i < len && (css[i] === " " || css[i] === "\n" || css[i] === "\r")) i++;
        runStart = i;
        continue;
      }
      // @layer block — unwrap, keeping inner rules
      i = braceIdx + 1;
      const innerStart = i;
      let depth = 1;
      while (i < len && depth > 0) {
        if (css[i] === "{") depth++;
        else if (css[i] === "}") depth--;
        if (depth > 0) i++;
      }
      out.push(css.slice(innerStart, i));
      if (i < len) i++;
      runStart = i;
      continue;
    }
    i++;
  }

  if (runStart < len) out.push(css.slice(runStart, len));
  return out.join("");
}

/**
 * Scope all CSS selectors under [data-rw-viewer] and strip @layer wrappers.
 */
function scopeCss(): Plugin {
  let outDir: string;
  return {
    name: "scope-css",
    configResolved(config) {
      outDir = config.build.outDir;
    },
    async writeBundle() {
      const cssPath = resolve(outDir, "embed.css");
      const code = await readFile(cssPath);
      const result = transform({
        filename: "embed.css",
        code,
        visitor: {
          Selector(selector) {
            if (hasScope(selector)) return selector;
            if (selector.length > 0 && isGlobal(selector[0])) {
              return [SCOPE, ...selector.slice(1)];
            }
            if (startsWithRootState(selector)) {
              return [
                [SCOPE, ...selector],
                [SCOPE, DESCENDANT, ...selector],
              ];
            }
            return [SCOPE, DESCENDANT, ...selector];
          },
        },
      });
      await writeFile(cssPath, stripLayers(result.code.toString()));
    },
  };
}

export default defineConfig({
  plugins: [
    svelte(),
    tailwindcss(),
    dts({
      // The tsconfig also covers e2e specs, vite configs and tests, and
      // `files: ["dist/lib/"]` publishes every `.d.ts` that lands here.
      include: ["src/**/*.ts", "src/**/*.svelte"],
      exclude: ["src/**/*.test.ts", "src/**/__fixtures__/**"],
      // Keep per-module declarations. Enabling `bundleTypes` requires installing
      // `@microsoft/api-extractor`; the build fails without it.
      // The plugin prints type errors but still emits and exits 0.
      afterDiagnostic(diagnostics) {
        if (diagnostics.length > 0) {
          throw new Error(`vite-plugin-dts: ${diagnostics.length} type error(s), printed above`);
        }
      },
    }),
    scopeCss(),
    licenseNotice(
      resolve(__dirname, "dist/lib/THIRD-PARTY-JS.md"),
      resolve(__dirname, "package.json"),
    ),
  ],
  resolve: {
    alias: {
      $lib: resolve(__dirname, "src/lib"),
    },
  },
  build: {
    lib: {
      entry: "src/embed.ts",
      formats: ["es"],
      fileName: "embed",
    },
    outDir: "dist/lib",
    sourcemap: true,
  },
});
