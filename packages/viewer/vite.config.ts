import { fileURLToPath } from "url";
import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import tailwindcss from "@tailwindcss/vite";
import { fontPreload } from "./vite-plugin-font-preload";

const __dirname = fileURLToPath(new URL(".", import.meta.url));

export default defineConfig({
  plugins: [svelte(), tailwindcss(), fontPreload()],
  build: {
    sourcemap: true,
    rollupOptions: {
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
      exclude: ["src/**/*.test.ts", "src/**/*.test.svelte.ts", "src/main.ts"],
    },
  },
});
