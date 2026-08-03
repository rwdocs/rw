export default {
  workspaces: {
    ".": {
      // No JS consumer exists — rw-embedded-preview reaches this through
      // `include_str!`. Named exactly rather than by glob: an entry is exempt
      // from both the unused-file and unused-export checks, so a sibling
      // matched by a glob would go unexamined rather than reported.
      entry: ["crates/rw-embedded-preview/src/preview.js"],
      // A server route. knip resolves a leading-slash specifier from the
      // filesystem root, so it never reaches the asset rw-assets serves —
      // building the asset or un-ignoring `dist/` does not change that.
      ignoreUnresolved: ["/lib/embed.js"],
    },
    "packages/core": {
      // index.js is napi output. Its loader probes every platform napi knows,
      // not just the targets this repo builds.
      ignoreDependencies: [/^@rwdocs\/core-/],
      // The wasi branch of that same loader. No wasi target is configured, so
      // the file it reaches for is never generated.
      ignoreUnresolved: ["./core.wasi.cjs"],
    },
  },
};
