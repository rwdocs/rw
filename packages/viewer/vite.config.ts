import { fileURLToPath } from "url";
import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import tailwindcss from "@tailwindcss/vite";

const __dirname = fileURLToPath(new URL(".", import.meta.url));

export default defineConfig({
  plugins: [svelte(), tailwindcss()],
  build: {
    sourcemap: true,
    rollupOptions: {
      input: {
        main: __dirname + "index.html",
        techdocs: __dirname + "src/techdocs.ts",
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
    include: ["src/**/*.test.ts"],
    coverage: {
      provider: "v8",
      reporter: ["text", "html"],
      include: ["src/**/*.ts", "src/**/*.svelte"],
      exclude: ["src/**/*.test.ts", "src/main.ts"],
    },
  },
});
