import eslintPluginBetterTailwindcss from "eslint-plugin-better-tailwindcss";
import { defineConfig } from "eslint/config";
import eslintParserSvelte from "svelte-eslint-parser";
import tseslint from "typescript-eslint";

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
      "better-tailwindcss/no-unknown-classes": [
        "error",
        {
          ignore: ["^animate-", "^breadcrumb-"],
        },
      ],
    },
    files: ["**/*.svelte"],
    languageOptions: {
      parser: eslintParserSvelte,
      parserOptions: {
        parser: tseslint.parser,
      },
    },
  },
]);
