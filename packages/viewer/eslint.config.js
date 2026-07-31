import eslintPluginBetterTailwindcss from "eslint-plugin-better-tailwindcss";
import boundaries from "eslint-plugin-boundaries";
import playwright from "eslint-plugin-playwright";
import svelte from "eslint-plugin-svelte";
import vitest from "@vitest/eslint-plugin";
import { defineConfig } from "eslint/config";
import eslintParserSvelte from "svelte-eslint-parser";
import tseslint from "typescript-eslint";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

// Type-aware rules need a TypeScript program, which `projectService` builds
// from the nearest tsconfig. `.svelte` has to be named explicitly: the project
// service skips non-standard extensions, and without this every component
// fails to parse rather than merely going unchecked.
const typeAwareParserOptions = {
  projectService: true,
  tsconfigRootDir: import.meta.dirname,
  extraFileExtensions: [".svelte"],
};

// Shared language options for every Svelte config block — ESLint flat
// config does not inherit across blocks, so each scope re-declares them.
const svelteLanguageOptions = {
  parser: eslintParserSvelte,
  parserOptions: {
    parser: tseslint.parser,
    ...typeAwareParserOptions,
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
          // Kit layers — strict isolation from domain.
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
          // Domain layers.
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

export default defineConfig([
  {
    ignores: ["coverage/**", "dist/**"],
  },
  // TypeScript baseline. `src` and the root-level build configs share this
  // block; `.svelte` and the e2e suite each get their own below, because both
  // layer further rules on top.
  {
    extends: [tseslint.configs.recommendedTypeChecked],
    files: ["src/**/*.{ts,svelte.ts}", "*.config.ts", "vite-plugin-*.ts"],
    languageOptions: { parserOptions: typeAwareParserOptions },
  },
  // Playwright's own rules are the point here: a missing `await` on an
  // assertion passes vacuously, and a fixed `waitForTimeout` is the usual
  // reason a spec is green alone and flaky in parallel.
  {
    extends: [tseslint.configs.recommendedTypeChecked, playwright.configs["flat/recommended"]],
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
      // Its six reports are a colour parser and `if (!box) throw` guards over a
      // nullable bounding box. Both fail loudly, which is the opposite of the
      // silently-skipped assertion the rule targets — and that case stays
      // covered, because `no-conditional-expect` remains an error.
      "playwright/no-conditional-in-test": "off",
      // The rule identifies page objects by a regex over the receiver's
      // identifier (`/^(page|frame)/`), so it has exactly one hit here:
      // `pageReplyBox.press("Escape")` — a locator, flagged for its name.
      // Renaming that variable would let the rule back on.
      "playwright/prefer-locator": "off",
    },
  },
  {
    extends: [tseslint.configs.recommendedTypeChecked],
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
    languageOptions: svelteLanguageOptions,
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
      // Fires on every `new Set()` / `new Map()` / `new URL()` in a rune file,
      // but it is aimed at mutating a collection *in place* while reading it
      // reactively. This codebase never does: `navigation.collapsed` copies,
      // mutates the copy and reassigns, `comments` holds its collections in
      // `$state.raw` (reassign-only by contract), and the rest are local
      // bookkeeping that no template reads. `SvelteSet`/`SvelteMap` would add
      // a proxy for reactivity that reassignment already provides.
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
