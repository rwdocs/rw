---
description: Rust coding conventions
globs: "**/*.rs"
---

# Rust Conventions

- Import all types and traits you use via `use` statements at the top of the file.
- Fully-qualified paths are only for disambiguating name conflicts or one-off references in doc comments.
- Prefer standard traits over custom methods that mirror them. Use `impl From<&str>` (or `impl FromStr` if it can fail) for string-to-type conversion instead of `fn parse`; use `impl Display` instead of `fn as_str` / `fn to_string`. Custom method names hide intent and don't compose with `format!`, `.parse()`, or `.into()`.
