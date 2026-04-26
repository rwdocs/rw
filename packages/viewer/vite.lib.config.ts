import { defineConfig, type Plugin } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import tailwindcss from "@tailwindcss/vite";
import dts from "vite-plugin-dts";
import { readFile, writeFile } from "fs/promises";
import { resolve } from "path";
import { fileURLToPath } from "url";
import { transform, type SelectorComponent, type Selector } from "lightningcss";

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
            return [SCOPE, DESCENDANT, ...selector];
          },
        },
      });
      await writeFile(cssPath, stripLayers(result.code.toString()));
    },
  };
}

export default defineConfig({
  plugins: [svelte(), tailwindcss(), dts({ rollupTypes: true }), scopeCss()],
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
