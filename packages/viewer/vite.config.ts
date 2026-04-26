/// <reference types="vitest" />
import { resolve } from "path";
import { fileURLToPath } from "url";
import { defineConfig } from "vite";
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
      "/api": "http://localhost:7979",
    },
  },
  test: {
    environment: "jsdom",
    globals: true,
    include: ["src/**/*.test.ts", "src/**/*.test.svelte.ts"],
    coverage: {
      provider: "v8",
      reporter: ["text", "html"],
      include: ["src/**/*.ts", "src/**/*.svelte"],
      exclude: [
        "src/**/*.test.ts",
        "src/**/*.test.svelte.ts",
        "src/**/__fixtures__/**",
        "src/main.ts",
      ],
    },
  },
});
