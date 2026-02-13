# Design: Migrate techdocs template.rs to minijinja

Replace hand-rolled `push_str`/`write!` HTML generation in `rw-techdocs/src/template.rs`
with a minijinja template. The current ~320 lines of Rust string operations become ~150
lines of readable HTML with Jinja2 syntax.

## Approach

Single monolithic template embedded as `const TEMPLATE: &str` in the Rust source. Compiled
into the binary — no external files at runtime.

## What Changes

**template.rs:**
- Remove all `render_*` functions and the `escape()` helper
- Add `const TEMPLATE: &str` with the full HTML as Jinja2 template
- Keep data structs (`PageData`, `NavItemData`, etc.) but derive `Serialize`
- `render_page()` creates `Environment`, adds template, renders with `PageData` context

**Template structure:**
```jinja
{# Recursive macro for nav items #}
{% macro nav_item(item) %}...{% endmacro %}

{# Macro for nav groups #}
{% macro nav_groups(groups) %}...{% endmacro %}

<!DOCTYPE html>
<html lang="en">
<head>...</head>
<body>
  <aside>sidebar with nav_groups macro</aside>
  <main>{{ html_content|safe }}</main>
  <aside>toc sidebar</aside>
</body>
</html>
```

**Cargo.toml:** add `minijinja = "2"`, add `serde` derive on data types.

**builder.rs:** no changes. Already constructs `PageData` and calls `template::render_page()`.

## What Doesn't Change

- HTML output is identical (same classes, same DOM structure)
- Public API of the template module (`render_page`, data structs)
- All existing tests pass without modification
- `html_content` passed through unescaped (`|safe`) since it's pre-rendered HTML

## Auto-escaping

minijinja auto-escapes by default, replacing the custom `escape()` function.

## Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Template engine | minijinja | Lightweight, Jinja2 syntax, good Rust integration |
| Template location | Embedded `const &str` | Self-contained binary, no runtime file deps |
| Template structure | Single file with macros | ~150 lines, easy to read top-to-bottom |
| Recursive nav | `{% macro %}` | Cleaner than Rust recursive fn with push_str |
