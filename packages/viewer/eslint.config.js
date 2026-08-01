import eslintPluginBetterTailwindcss from "eslint-plugin-better-tailwindcss";
import boundaries from "eslint-plugin-boundaries";
import playwright from "eslint-plugin-playwright";
import svelte from "eslint-plugin-svelte";
import vitest from "@vitest/eslint-plugin";
import js from "@eslint/js";
import { defineConfig } from "eslint/config";
import globals from "globals";
import eslintParserSvelte from "svelte-eslint-parser";
import tseslint from "typescript-eslint";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

// `.svelte` has to be named explicitly: the project service skips non-standard
// extensions, and without this every component fails to parse rather than
// merely going unchecked.
const typeAwareParserOptions = {
  projectService: true,
  tsconfigRootDir: import.meta.dirname,
  extraFileExtensions: [".svelte"],
};

// Blocks matching the same file merge, so only the first block matching a
// scope needs to set these.
const svelteLanguageOptions = {
  parser: eslintParserSvelte,
  parserOptions: {
    parser: tseslint.parser,
    ...typeAwareParserOptions,
  },
};

// Gradient stops, shadow and decoration colors, ring / outline, divide /
// placeholder / caret / fill / stroke can each smuggle a raw palette color into
// a component, so the group reaches well past `bg|text|border`.
const COLOR_PREFIX_GROUP =
  "bg|text|border|ring|outline|fill|stroke|divide|placeholder|caret|accent|decoration|shadow|from|via|to";

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

// Layer dependency config per design-kit spec §2.2. Applied by a single block
// so the settings are evaluated once; the parsers come from the Svelte and
// TypeScript blocks that match the same files.
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
      { type: "entry", pattern: "src/{App.svelte,embed.ts,main.ts}", mode: "file" },
    ],
    "boundaries/ignore": ["src/**/*.test.ts", "src/**/__fixtures__/**"],
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
        policies: [
          {
            from: { element: { type: "kit-hooks" } },
            allow: { to: { element: { type: ["kit-hooks", "kit-root"] } } },
          },
          {
            from: { element: { type: "kit-primitives" } },
            allow: { to: { element: { type: ["kit-primitives", "kit-hooks", "kit-root"] } } },
          },
          {
            from: { element: { type: "kit-root" } },
            allow: { to: { element: { type: "kit-root" } } },
          },
          {
            from: { element: { type: "rw-context" } },
            allow: { to: { element: { type: ["state", "api", "types"] } } },
          },
          {
            from: { element: { type: "domain-lib" } },
            allow: {
              to: {
                element: {
                  type: ["domain-lib", "types", "kit-primitives", "kit-hooks", "kit-root"],
                },
              },
            },
          },
          {
            from: { element: { type: "state" } },
            allow: {
              to: {
                element: {
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
          },
          { from: { element: { type: "components" } }, allow: { to: { element: { type: "*" } } } },
          { from: { element: { type: "pages" } }, allow: { to: { element: { type: "*" } } } },
          { from: { element: { type: "entry" } }, allow: { to: { element: { type: "*" } } } },
          {
            from: { element: { type: "api" } },
            allow: { to: { element: { type: ["api", "types"] } } },
          },
          { from: { element: { type: "types" } }, allow: { to: { element: { type: "types" } } } },
        ],
      },
    ],
  },
};

// typescript-eslint switches off the base rules it replaces, so it must stay
// second. An entry appended here does not win everywhere — the e2e and
// `.svelte` blocks each extend something after it.
const TS_BASELINE = [js.configs.recommended, tseslint.configs.recommendedTypeChecked];

export default defineConfig([
  {
    ignores: ["coverage/**", "dist/**"],
  },
  // Both default to non-blocking. Set here rather than via `--max-warnings 0`,
  // which would also promote the better-tailwindcss rules that are meant to
  // stay non-blocking.
  {
    linterOptions: {
      reportUnusedDisableDirectives: "error",
      reportUnusedInlineConfigs: "error",
    },
  },
  // No `.js` is in `tsconfig.json`, so ESLint is their only checker — hence the
  // globals, without which `no-undef` misfires on `process`. Do not narrow the
  // glob to `*.js`: a script added outside the package root would get nothing.
  {
    extends: [js.configs.recommended],
    files: ["**/*.js"],
    languageOptions: { globals: globals.node },
  },
  {
    extends: TS_BASELINE,
    files: ["src/**/*.{ts,svelte.ts}", "*.config.ts", "vite-plugin-*.ts"],
    languageOptions: { parserOptions: typeAwareParserOptions },
  },
  // Playwright's own rules are the point here: a missing `await` on an
  // assertion passes vacuously, and a fixed `waitForTimeout` is the usual
  // reason a spec is green alone and flaky in parallel.
  {
    extends: [...TS_BASELINE, playwright.configs["flat/recommended"]],
    files: ["e2e/**/*.ts"],
    languageOptions: { parserOptions: typeAwareParserOptions },
    rules: {
      // The plugin ships these at `warn`, which gates nothing. Listed rather
      // than mapped so the set is greppable and a plugin upgrade that adds a
      // rule leaves it at the plugin's own severity until someone decides.
      // Its other rules are already `error`, except `no-empty-pattern`, which
      // it turns off on purpose — Playwright's `async ({}, testInfo) =>` is an
      // empty pattern — and `consistent-spacing-between-blocks`, autofixable
      // whitespace we leave non-blocking.
      "playwright/expect-expect": "error",
      "playwright/max-nested-describe": "error",
      "playwright/no-conditional-expect": "error",
      "playwright/no-duplicate-hooks": "error",
      "playwright/no-duplicate-slow": "error",
      "playwright/no-element-handle": "error",
      "playwright/no-eval": "error",
      "playwright/no-force-option": "error",
      "playwright/no-nested-step": "error",
      "playwright/no-page-pause": "error",
      "playwright/no-skipped-test": "error",
      "playwright/no-useless-await": "error",
      "playwright/no-useless-not": "error",
      "playwright/no-wait-for-selector": "error",
      "playwright/no-wait-for-timeout": "error",
      "playwright/prefer-hooks-in-order": "error",
      "playwright/prefer-hooks-on-top": "error",
      "playwright/prefer-to-have-count": "error",
      "playwright/prefer-to-have-length": "error",
      // Its hits here are guards that throw, which is the opposite of the
      // silently-skipped assertion the rule targets. That case stays covered by
      // `no-conditional-expect`.
      "playwright/no-conditional-in-test": "off",
      // The rule identifies page objects by a regex over the receiver's
      // identifier (`/^(page|frame)/`), so a locator whose variable name starts
      // with `page` is flagged for its name alone. Rename those to re-enable.
      "playwright/prefer-locator": "off",
    },
  },
  // Components: the same TypeScript baseline as `src`, with the exceptions
  // below. eslint-plugin-svelte's own rules are applied by a separate block.
  {
    extends: [
      ...TS_BASELINE,
      // Base rules tsc covers. Its own glob is `**/*.ts`, which would intersect
      // this block's away, so the rules are taken directly. Must precede the
      // block's `rules` — it sets `prefer-const: "error"`. Its `no-unreachable`
      // entry only holds because `tsconfig.json` sets `allowUnreachableCode`.
      { rules: tseslint.configs.eslintRecommended.rules },
    ],
    files: ["src/**/*.svelte"],
    languageOptions: svelteLanguageOptions,
    rules: {
      // Svelte 5 runes must be declared with `let` — the compiler rewrites
      // `let { x } = $props()` and `let x = $state(0)` behind the scenes, so
      // the source never reassigns them and `prefer-const` flags every one.
      "prefer-const": "off",
      // Component prop types do not reach template expressions. The TypeScript
      // program is built from the raw `.svelte` source, while the types for
      // `<Child onPick={(id) => …} />` only exist in svelte2tsx's output, so
      // every callback parameter in markup is `any` and this family reports on
      // all of them. `svelte-check` runs through svelte2tsx and does check
      // these properly. The rules stay on for `.ts`, where they work.
      "@typescript-eslint/no-unsafe-argument": "off",
      "@typescript-eslint/no-unsafe-assignment": "off",
      "@typescript-eslint/no-unsafe-call": "off",
      "@typescript-eslint/no-unsafe-member-access": "off",
      "@typescript-eslint/no-unsafe-return": "off",
    },
  },
  // Svelte-specific correctness — reactivity, keyed `{#each}`, template a11y.
  // These sit outside what `svelte-check` and typescript-eslint can see: a
  // plain `Set` mutated inside a rune type-checks and compiles, it just never
  // triggers an update.
  {
    extends: [svelte.configs.recommended],
    files: ["src/**/*.svelte"],
  },
  // Rune modules carry the same reactivity rules as components. The plugin's
  // own `recommended` leaves its rule block unscoped, so a `files` glob of
  // `*.svelte` alone would silently skip these — and shared state is exactly
  // where a non-reactive `Set` or `Map` does the most damage.
  {
    extends: [svelte.configs.recommended],
    files: ["src/**/*.svelte.ts"],
  },
  {
    files: ["src/**/*.{svelte,svelte.ts}"],
    rules: {
      // `linterOptions.reportUnusedDisableDirectives` reads JS comments only.
      // Markup directives are this plugin's, and it reports a stale one only
      // when asked.
      "svelte/comment-directive": ["error", { reportUnusedDisableDirectives: true }],
      // Aimed at mutating a collection in place while reading it reactively.
      // This code reassigns instead, which already triggers the update, so
      // `SvelteSet`/`SvelteMap` would add a proxy for nothing.
      "svelte/prefer-svelte-reactivity": "off",
    },
  },
  {
    files: ["src/**/*.{test,spec}.ts"],
    plugins: { vitest },
    extends: [vitest.configs.recommended],
    rules: {
      // `expect(obj.method).toHaveBeenCalled()` reads the method without
      // calling it, which the base rule cannot distinguish from a genuine
      // unbound reference. The Vitest port understands `expect` and still
      // reports the real ones, so swap rather than switch off.
      "@typescript-eslint/unbound-method": "off",
      "vitest/unbound-method": "error",
      // `vi.fn(async () => ({}))` needs `async` to satisfy the mocked
      // signature's `Promise<T>`; the rule's escape hatch is to return a
      // promise explicitly, which only makes the doubles harder to read.
      "@typescript-eslint/require-await": "off",
    },
  },
  {
    extends: [eslintPluginBetterTailwindcss.configs.recommended],
    settings: {
      "better-tailwindcss": {
        entryPoint: "src/app.css",
      },
    },
    rules: {
      "better-tailwindcss/no-unknown-classes": [
        "error",
        {
          ignore: [
            "layout-*",
            "drawer-flow-*",
            "comment-body",
            "thread-card",
            "diagram-zoom-*",
            "rw-button-group",
          ],
        },
      ],
      "better-tailwindcss/enforce-consistent-line-wrapping": [
        "warn",
        { printWidth: 100, strictness: "loose", preferSingleLine: true },
      ],
      "better-tailwindcss/enforce-consistent-variant-order": "warn",
    },
    // Wider than the block that sets the parser, so it sets its own: a
    // component outside `src/` would otherwise fail to parse.
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
  // Kit components must reach for the semantic layer only (bg-bg-*, text-fg-*,
  // border-*-border, text-{intent}-fg), never a raw palette utility or one of
  // our primitive scales.
  {
    files: ["src/lib/ui/**/*.svelte"],
    rules: {
      "better-tailwindcss/no-restricted-classes": [
        "error",
        {
          restrict: [
            {
              pattern: `^(${COLOR_PREFIX_GROUP})-(${TAILWIND_HUES.join("|")})-(${PALETTE_STEPS})$`,
              message:
                "Use semantic tokens (bg-bg-*, text-fg-*, border-*-border) instead of raw palette utilities.",
            },
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
