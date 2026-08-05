import js from "@eslint/js";
import { defineConfig, includeIgnoreFile } from "eslint/config";
import globals from "globals";
import { fileURLToPath } from "node:url";

// Flat config reads no .gitignore of its own, so the repo's is pulled in rather
// than restated here. Build output and worktree checkouts are declared once, in
// the place every other tool already reads.
const gitignore = includeIgnoreFile(fileURLToPath(new URL(".gitignore", import.meta.url)));

export default defineConfig([
  gitignore,
  {
    ignores: [
      // Has its own config and lint script. Removing this does not defer to
      // them: ESLint resolves the nearest config per file, so the package would
      // be linted a second time under that same config, for nothing.
      "packages/viewer/**",
      // Rewritten by every `napi build`; nothing in it is authored.
      "packages/core/index.js",
    ],
  },
  // Everything hand-written that no other block claims. Deliberately a catch-all
  // rather than a list of known directories: a file matching no block is still
  // walked and still reported, just with no rules — narrowing here fails silently.
  {
    extends: [js.configs.recommended],
    files: ["**/*.{js,mjs,cjs}"],
    ignores: ["crates/rw-embedded-preview/src/**/*.{js,mjs,cjs}"],
    languageOptions: { globals: globals.node },
  },
  // The preview shell, served to browsers via `include_str!`. It is in no
  // tsconfig, so `no-undef` is the only check standing between a typo and a
  // runtime error — which is why it gets browser globals rather than the node
  // ones above, and why the two sets must not merge.
  {
    extends: [js.configs.recommended],
    files: ["crates/rw-embedded-preview/src/**/*.{js,mjs,cjs}"],
    languageOptions: { globals: globals.browser },
  },
]);
