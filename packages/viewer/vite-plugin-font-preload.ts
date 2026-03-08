import type { HtmlTagDescriptor, Plugin } from "vite";

/**
 * Vite plugin that injects <link rel="preload"> tags for critical font files
 * into index.html, eliminating the flash of unstyled text (FOUT) caused by
 * fonts loading after JS/CSS.
 *
 * Cyrillic and Latin subset woff2 files are preloaded — they cover the most
 * common glyphs and are small enough that preloading doesn't hurt performance.
 */

const PRELOAD_FONTS = [
  "roboto-cyrillic-400-normal",
  "roboto-cyrillic-500-normal",
  "roboto-cyrillic-700-normal",
  "roboto-latin-400-normal",
  "roboto-latin-500-normal",
  "roboto-latin-700-normal",
  "jetbrains-mono-latin-400-normal",
];

export function fontPreload(): Plugin {
  return {
    name: "font-preload",

    transformIndexHtml: {
      order: "post",
      handler(_html, ctx) {
        if (ctx.server) {
          return PRELOAD_FONTS.map((name): HtmlTagDescriptor => {
            const pkg = name.startsWith("roboto")
              ? "@fontsource/roboto"
              : "@fontsource/jetbrains-mono";
            return {
              tag: "link",
              attrs: {
                rel: "preload",
                as: "font",
                type: "font/woff2",
                crossorigin: "",
                href: `/node_modules/${pkg}/files/${name}.woff2`,
              },
              injectTo: "head",
            };
          });
        }

        const bundle = ctx.bundle;
        if (!bundle) return [];

        return PRELOAD_FONTS.flatMap((name): HtmlTagDescriptor[] => {
          const asset = Object.keys(bundle).find((k) => k.includes(name) && k.endsWith(".woff2"));
          if (!asset) return [];
          return [
            {
              tag: "link",
              attrs: {
                rel: "preload",
                as: "font",
                type: "font/woff2",
                crossorigin: "",
                href: `/${asset}`,
              },
              injectTo: "head",
            },
          ];
        });
      },
    },
  };
}
