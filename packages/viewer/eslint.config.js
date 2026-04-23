import eslintPluginBetterTailwindcss from "eslint-plugin-better-tailwindcss";
import { defineConfig } from "eslint/config";
import eslintParserSvelte from "svelte-eslint-parser";
import tseslint from "typescript-eslint";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

// Shared language options for every Svelte config block — ESLint flat
// config does not inherit across blocks, so each scope re-declares them.
const svelteLanguageOptions = {
  parser: eslintParserSvelte,
  parserOptions: {
    parser: tseslint.parser,
  },
};

// Prefix list for color-bearing utility classes. Broader than `bg|text|border`
// because gradient stops (from/via/to), shadow / decoration colors, ring /
// outline colors, and divide / placeholder / caret / fill / stroke can all
// smuggle a raw palette color into a component.
const COLOR_PREFIX_GROUP =
  "bg|text|border|ring|outline|fill|stroke|divide|placeholder|caret|accent|decoration|shadow|from|via|to";

// Step-set Tailwind generates for each hue.
const PALETTE_STEPS = "50|100|200|300|400|500|600|700|800|900|950";

// Discover Tailwind 4's default color-family names from its bundled theme.css
// so the denylist tracks Tailwind upgrades automatically. Matches only
// numbered steps (--color-red-500, etc.), so white/black/transparent/current
// remain allowed as low-risk escape hatches. Resolved via import.meta.resolve
// because tailwindcss is hoisted to the workspace root in this monorepo.
const tailwindThemeCss = readFileSync(
  fileURLToPath(import.meta.resolve("tailwindcss/theme.css")),
  "utf8",
);
const TAILWIND_HUES = [
  ...new Set([...tailwindThemeCss.matchAll(/--color-([a-z]+)-\d+:/g)].map((m) => m[1])),
].sort();

// Our own primitive scales declared in lib/ui/tokens/colors.css.
const OUR_PRIMITIVE_SCALES = "accent|info|success|warning|danger|attention";
const OUR_PRIMITIVE_STEPS = "50|100|500|600|700";

export default defineConfig([
  {
    ignores: ["coverage/**", "dist/**"],
  },
  {
    extends: [eslintPluginBetterTailwindcss.configs.recommended],
    settings: {
      "better-tailwindcss": {
        entryPoint: "src/app.css",
      },
    },
    rules: {
      "better-tailwindcss/no-unknown-classes": ["error", { ignore: ["layout-*", "drawer-flow-*"] }],
      "better-tailwindcss/enforce-consistent-line-wrapping": [
        "warn",
        { printWidth: 100, strictness: "loose", preferSingleLine: true },
      ],
    },
    files: ["**/*.svelte"],
    languageOptions: svelteLanguageOptions,
  },
  // Design-kit guardrail: forbid raw Tailwind palette utilities AND our own
  // primitive tokens inside `src/lib/ui/**`. Kit components must use only the
  // semantic layer (bg-bg-*, text-fg-*, border-*-border, text-accent-fg,
  // text-{intent}-fg etc.). Phase 3 widens the glob to `src/components/**`.
  {
    files: ["src/lib/ui/**/*.svelte"],
    languageOptions: svelteLanguageOptions,
    rules: {
      "better-tailwindcss/no-restricted-classes": [
        "error",
        {
          restrict: [
            // Tailwind default palette — hue list generated from theme.css.
            {
              pattern: `^(${COLOR_PREFIX_GROUP})-(${TAILWIND_HUES.join("|")})-(${PALETTE_STEPS})$`,
              message:
                "Use semantic tokens (bg-bg-*, text-fg-*, border-*-border) instead of raw palette utilities.",
            },
            // Our own primitive tokens — declared as @theme so Tailwind emits
            // .bg-accent-500 etc., but kit components must consume semantic
            // tokens (bg-accent-bg, text-{intent}-fg) not the primitives.
            {
              pattern: `^(${COLOR_PREFIX_GROUP})-(${OUR_PRIMITIVE_SCALES})-(${OUR_PRIMITIVE_STEPS})$`,
              message:
                "Use semantic tokens (bg-accent-bg, text-{intent}-fg, border-{intent}-border) instead of primitive scales.",
            },
          ],
        },
      ],
    },
  },
]);
