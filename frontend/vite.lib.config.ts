import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import tailwindcss from "@tailwindcss/vite";

// Svelte runtime is intentionally bundled (not externalized) so that
// non-Svelte host applications (e.g. React-based Backstage) can use
// the library without adding Svelte as a dependency.
export default defineConfig({
  plugins: [svelte(), tailwindcss()],
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
