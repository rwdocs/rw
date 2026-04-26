import eslintPluginBetterTailwindcss from "eslint-plugin-better-tailwindcss";
import boundaries from "eslint-plugin-boundaries";
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

// Layer dependency config per design-kit spec §2.2. Used in a single block
// matching .ts/.svelte/.svelte.ts; the .svelte parser is set by the existing
// svelte block above, and a thin block below sets the TS parser for .ts files
// — flat config merges configs across matching blocks so rules + parser
// combine without the boundaries settings being evaluated twice.
const boundariesConfig = {
  plugins: { boundaries },
  settings: {
    "boundaries/elements": [
      // More-specific patterns first — first match wins.
      { type: "kit-tokens", pattern: "src/lib/ui/tokens/**" },
      { type: "kit-hooks", pattern: "src/lib/ui/hooks/**" },
      { type: "kit-primitives", pattern: "src/lib/ui/primitives/**" },
      { type: "kit-root", pattern: "src/lib/ui/*.{ts,svelte}", mode: "file" },
      { type: "rw-context", pattern: "src/lib/context.ts", mode: "file" },
      { type: "domain-lib", pattern: "src/lib/*.{ts,svelte}", mode: "file" },
      { type: "state", pattern: "src/state/**" },
      { type: "components", pattern: "src/components/**" },
      { type: "pages", pattern: "src/pages/**" },
      { type: "api", pattern: "src/api/**" },
      { type: "types", pattern: "src/types/**" },
      // Top-level entry points wire everything together.
      { type: "entry", pattern: "src/{App.svelte,embed.ts,index.ts,main.ts}", mode: "file" },
    ],
    "boundaries/ignore": ["src/**/*.test.ts", "src/**/*.test.svelte.ts", "src/**/__fixtures__/**"],
    "boundaries/include": ["src/**/*.{ts,svelte,svelte.ts}"],
    "import/resolver": {
      typescript: { project: "./tsconfig.json" },
    },
  },
  rules: {
    "boundaries/dependencies": [
      "error",
      {
        default: "disallow",
        rules: [
          // Kit layers — strict isolation from domain.
          {
            from: { type: "kit-hooks" },
            allow: { to: { type: ["kit-hooks", "kit-root"] } },
          },
          {
            from: { type: "kit-primitives" },
            allow: { to: { type: ["kit-primitives", "kit-hooks", "kit-root"] } },
          },
          { from: { type: "kit-root" }, allow: { to: { type: "kit-root" } } },
          // Domain layers.
          { from: { type: "rw-context" }, allow: { to: { type: ["state", "api", "types"] } } },
          {
            from: { type: "domain-lib" },
            allow: {
              to: { type: ["domain-lib", "types", "kit-primitives", "kit-hooks", "kit-root"] },
            },
          },
          {
            from: { type: "state" },
            allow: {
              to: {
                type: [
                  "state",
                  "domain-lib",
                  "rw-context",
                  "types",
                  "api",
                  "kit-primitives",
                  "kit-hooks",
                  "kit-root",
                ],
              },
            },
          },
          { from: { type: "components" }, allow: { to: { type: "*" } } },
          { from: { type: "pages" }, allow: { to: { type: "*" } } },
          { from: { type: "entry" }, allow: { to: { type: "*" } } },
          { from: { type: "api" }, allow: { to: { type: ["api", "types"] } } },
          { from: { type: "types" }, allow: { to: { type: "types" } } },
        ],
      },
    ],
  },
};

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
  // Layer dependency rules per design-kit spec §2.2. The kit (`src/lib/ui/**`)
  // must remain free of RW domain knowledge so it can be lifted into a
  // standalone package later. `src/lib/context.ts` is the documented composition
  // root and gets its own element type so it may import state shapes.
  {
    ...boundariesConfig,
    files: ["src/**/*.{ts,svelte,svelte.ts}"],
  },
  {
    files: ["src/**/*.{ts,svelte.ts}"],
    languageOptions: { parser: tseslint.parser },
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
