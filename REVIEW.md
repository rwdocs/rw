# Review: `rw techdocs build`

Tested with `rw techdocs build -c ~/Projects/invoices-migration/arch/rw.toml`,
served output with `python3 -m http.server` and compared against `rw serve`.

## ~~1. Font files not copied (404 errors)~~ FIXED

~~Only `styles.css` is written to `assets/`. All `.woff`/`.woff2` font files
(Roboto, JetBrains Mono) referenced by the CSS are missing. `load_frontend_css()`
reads only the CSS file; `write_css()` writes only that CSS. Font files from
`frontend/dist/assets/` are never copied.~~

~~**Result**: 12+ 404 errors per page load, fallback to system fonts.~~

Fixed: `write_css()` replaced with `write_assets()` which uses `rw-assets` crate
to copy CSS and all font files (`.woff`/`.woff2`) from the frontend dist to the
output `assets/` directory.

## ~~2. Navigation/breadcrumb/logo links are absolute paths~~ FIXED

~~All links start with `/`:~~
~~- Nav: `href="/domains/billing/adrs"`~~
~~- Breadcrumbs: `href="/domains/billing"`~~
~~- Logo: `href="/"`~~

~~When deployed to S3 under entity prefix (e.g., `default/Component/arch/`), every
link breaks — they navigate to the S3 bucket root instead of relative to the entity.~~

Fixed: `convert_nav_items`, `convert_breadcrumbs`, and logo in `builder.rs`/`template.rs`
now use `relative_path()` when `relative_links: true`. All links are relative to
each page's location.

## ~~3. SVG diagram links are absolute~~ FIXED

~~C4 PlantUML diagrams contain `xlink:href="/domains/billing/systems/billing-receipts"`
— also absolute. Same breakage on S3.~~

Fixed: `LinkConfig` on `DiagramProcessor` applies `relative_links` and `trailing_slash`
to `$link` URLs in C4 macro generation.

## ~~4. Scoped navigation missing section headers and scope context~~ FIXED

~~Compared to `rw serve`, the static build is missing:~~
~~- **"< Home" back link** at the top of scoped sections~~
~~- **Section title** (e.g., bold "Биллинг" header)~~
~~- **Section type labels** (e.g., "SYSTEMS" uppercase header that groups systems~~
~~  separately from other nav items)~~

~~The static build renders all scoped nav items as a flat list instead.~~

Fixed: `template.rs` now renders scope headers (back link + section title) and
groups nav items by `section_type` with uppercase labels (e.g., "DOMAINS",
"SYSTEMS"). `builder.rs` preserves `section_type` from `NavItem`, converts
`Navigation.scope`/`parent_scope` to `ScopeHeaderData`, and groups items via
`group_nav_items()` matching the frontend's `groupNavItems()` logic.

## 5. Tabs are non-functional

Tab content only shows the first tab. Clicking other tabs does nothing (no JS).
Since Backstage strips JS this is somewhat expected, but the content of inactive
tabs is completely hidden — users can never see it.
