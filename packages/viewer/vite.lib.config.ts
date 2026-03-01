import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import tailwindcss from "@tailwindcss/vite";
import dts from "vite-plugin-dts";

export default defineConfig({
  plugins: [svelte(), tailwindcss(), dts({ rollupTypes: true })],
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
