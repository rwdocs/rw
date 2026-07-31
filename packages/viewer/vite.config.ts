import { resolve } from "path";
import { fileURLToPath } from "url";
// `vitest/config` re-exports Vite's `defineConfig` with the `test` block
// typed. Vite's own export does not carry it, and the triple-slash
// `types="vitest"` reference that used to patch that in no longer does.
import { defineConfig } from "vitest/config";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { svelteTesting } from "@testing-library/svelte/vite";
import tailwindcss from "@tailwindcss/vite";
import { fontPreload } from "./vite-plugin-font-preload";

const __dirname = fileURLToPath(new URL(".", import.meta.url));

export default defineConfig({
  plugins: [svelte(), svelteTesting(), tailwindcss(), fontPreload()],
  resolve: {
    alias: {
      $lib: resolve(__dirname, "src/lib"),
    },
  },
  build: {
    sourcemap: true,
    rolldownOptions: {
      input: {
        main: __dirname + "index.html",
      },
    },
  },
  server: {
    proxy: {
      "/_api": "http://localhost:7979",
    },
  },
  test: {
    environment: "jsdom",
    globals: true,
    include: ["src/**/*.test.ts"],
    coverage: {
      provider: "v8",
      reporter: ["text", "html"],
      include: ["src/**/*.ts", "src/**/*.svelte"],
      exclude: ["src/**/*.test.ts", "src/**/__fixtures__/**", "src/main.ts"],
    },
  },
});
