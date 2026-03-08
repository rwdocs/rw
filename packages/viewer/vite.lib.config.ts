import { defineConfig, type Plugin } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import tailwindcss from "@tailwindcss/vite";
import dts from "vite-plugin-dts";
import { readFile, writeFile } from "fs/promises";
import { resolve } from "path";
import { transform, type SelectorComponent, type Selector } from "lightningcss";

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

/** Scope all CSS selectors in emitted assets under [data-rw-viewer]. */
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
      await writeFile(cssPath, result.code);
    },
  };
}

export default defineConfig({
  plugins: [svelte(), tailwindcss(), dts({ rollupTypes: true }), scopeCss()],
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
