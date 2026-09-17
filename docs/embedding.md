# Embedding

This page covers integration concerns for embedding `@rwdocs/viewer` (or rendering pages
through `@rwdocs/core`) in a host application: pointing `@rwdocs/core` at a site
with `projectDir`, resolving cross-entity links via `resolveSectionRefs`,
durable comment keys for hosts that store their own comments, and page attrs.

When you embed the viewer in a host application that stores **its own**
comments — for example the Backstage plugin pair — each comment needs a stable
identifier for the page it annotates.

## `projectDir` roots every path

`createSite({ projectDir })` in `@rwdocs/core` names the project root, and every
path RW uses is derived from it: the docs source directory, the `.rw/` directory
holding the render cache and the comments database, and the PlantUML `!include`
search directories. If `projectDir` contains an `rw.toml`, that file is loaded
and its relative paths resolve against `projectDir`; if it does not, defaults
rooted at `projectDir` are used.

RW does **not** walk up from `projectDir` looking for an `rw.toml` in a parent
directory, and it does not consult the Node process's working directory. A host
that mounts several sites can therefore point each `createSite` call at its own
directory without one site's configuration leaking into another.

## Consuming page attrs

Only the NAPI/core page boundary exposes [page-local `attrs`](metadata.md#attrs-page-local-integration-data):

```js
const page = await site.renderPage("guide");
const owner = page.meta.attrs?.owner;
if (typeof owner === "string") {
  // Validate against your integration's schema before using it.
}
```

The core declarations describe JSON values (including nested null), not `any`.
`meta.attrs` is omitted when empty, and never inherits. Use string identifiers
when exact large integers matter: JavaScript number precision applies. Rust JSON
byte roundtrips preserve the chosen supported finite `f64` values; they do not
remove JavaScript's numeric limits. Source validation bounds array/object
nesting so manifest/cache readers can load the values; see
[attrs validation](metadata.md#attrs-page-local-integration-data).
Attrs are ordinary data, including keys such as `__proto__`; do not treat them as
executable configuration or authorization rules. RW does not render or interpret them, and
the built-in HTTP API, viewer, search and navigation shapes are unchanged.

For S3, deploy version-1-compatible readers with attrs support before consumers
rely on published attrs. Older readers ignore and do not preserve them on
reserialization. Attributes increase all-site manifest/structure-cache payloads
and resident snapshot memory; selected-page conversion adds response work, not
new S3 object requests. Measure your own payloads and Site reuse/eviction cadence:
local benchmarks do not establish external-host latency.

Metadata on fresh and cached HTML responses comes from the applicable Site
snapshot. Keep the existing host refresh strategy; attrs add no polling, watcher
delivery, same-mtime or cross-process cache correctness guarantees. An attrs-only
snapshot change does not introduce whole-site HTML invalidation; normal source
mtime invalidation still applies. No new refresh API is provided.

## `resolveSectionRefs` must map the site-root ref

`mountRw({ resolveSectionRefs })` lets the viewer turn a cross-entity link (a
breadcrumb, back-link, or content link that points outside the current
entity's scope) into your host's own URL for that entity. The viewer resolves
a link by walking its target's section ancestry — nearest section first,
site-root section last — and using the first ancestor your resolver maps to a
base URL.

Because every ancestry chain ends at the site-root section ref (e.g.
`section:default/root`), **your `resolveSectionRefs` must return a base URL
for it**. That mapping is the guaranteed backstop every link ultimately
resolves against, however deeply nested the target section is. If you decline
to map the root ref (return `undefined`, `null`, or omit it), the viewer falls
back to resolving the link against the current mount's own base path — a
last-resort for a host that violates the contract, not something to rely on
for correct cross-entity navigation.

## Don't key on `path`

`PageMeta.path` is the **site-root-relative URL** of the page. It is convenient,
but it is **not stable**: remounting or moving a whole section to a different base
path changes the `path` of every page in that section. If your comments are keyed
on `path`, a section move orphans all of them even though the content is unchanged.

## Key on `(sectionRef, subpath)` instead

`PageMeta` also exposes:

- **`sectionRef`** — the section's identity, e.g. `domain:default/billing`.
- **`subpath`** — the page's path **relative to its section root**, e.g. `api`
  for the page at `domains/billing/api` inside section `domains/billing`. It is
  the empty string for a section's own root page, and the full page path for
  pages that fall outside any explicit section (these report the implicit root
  section).

The pair `(sectionRef, subpath)` is your durable comment key. It **survives a
whole-section remount**: the URL prefix changes, but each page's `subpath`
(relative to its unchanged section) does not.

```ts
const commentKey = `${meta.sectionRef}#${meta.subpath}`;
```

## What it does and does not survive

| Change | `(sectionRef, subpath)` survives? |
|--------|-----------------------------------|
| Mounting a whole section under a different base URL, its `sectionRef` unchanged | ✅ Yes |
| Relocating a whole section's directory while its effective name, kind, and namespace stay the same | ✅ Yes |
| Renaming a section directory while keeping explicit `name`, kind, and namespace | ✅ Yes |
| Changing a section's effective name (its `sectionRef` changes) | ❌ No |
| Moving a single page **within** a section (its `subpath` changes) | ❌ No |

An explicit [metadata `name`](metadata.md#name-identity-independent-of-the-documentation-path)
can preserve identity across a section-directory rename: declare the old derived
name before moving, and retain kind, namespace, and each page's relative subpath.
It does **not** stabilize intra-section page moves. Changing an effective name
changes the comment key; RW does not retain old-ref aliases or automatically
migrate existing comments. A durable per-page ID remains out of scope.

## Resolve section ancestry with `sectionAncestry`

Both the page response (`renderPage` / the page HTTP API) and the navigation
response (`getNavigation` / the navigation HTTP API) include a `sectionAncestry`
map, so a host can resolve a page's or view's full section context from a single
response instead of walking sections with follow-up calls.

- **Keys** are section refs (`kind:namespace/name`).
- **Values** are ancestry chains — arrays of `{ sectionRef, subpath }` anchors.
  Each chain **starts with the section itself** (empty `subpath`), then its
  ancestors nearest-first with the root section last.

A page's map covers the page's own section, every section it links to, and its
breadcrumb sections; a navigation view's map covers its items, scope, and parent
scope. Look up a page's own section by its `sectionRef`:

```js
// The ancestry chain of the page's own section, root last.
const chain = page.sectionAncestry[page.meta.sectionRef];
```

Over the HTTP API the field is **omitted when empty**; `@rwdocs/core` always
returns the map (empty `{}` when there is nothing to resolve).

## Sanitize the comment HTML you supply

If your host stores its own comments and supplies them through
`mountRw({ comments })` (an injected `CommentApiClient`), **you** are responsible
for sanitizing each comment's `bodyHtml`. The viewer renders that field as
**trusted HTML** — it injects it directly, with no client-side sanitization. The
default `rw serve` backend sanitizes comment markdown to a restricted CommonMark
subset before it ever reaches the viewer; an injected client bypasses that path
entirely.

Two safe options:

- **Render with `renderCommentBody`** from `@rwdocs/core`, which produces HTML in
  the same restricted subset the default backend uses. The Backstage backend
  plugin already does this.
- **Omit `bodyHtml`** and return only the plain-text `body` — the viewer renders
  it as text, with no HTML injection.

Returning unsanitized HTML — or proxying `bodyHtml` straight from an upstream
store — lets comment authors inject scripts that execute in your page's origin
(stored XSS).
