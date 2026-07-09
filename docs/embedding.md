# Embedding

This page covers two concerns for embedding `@rwdocs/viewer` (or rendering pages
through `@rwdocs/core`) in a host application: resolving cross-entity links via
`resolveSectionRefs`, and durable comment keys for hosts that store their own
comments.

When you embed the viewer in a host application that stores **its own**
comments — for example the Backstage plugin pair — each comment needs a stable
identifier for the page it annotates.

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
| Relocating a whole section's directory while its name (last path segment) stays the same | ✅ Yes |
| Renaming a section (its `sectionRef` changes) | ❌ No |
| Moving a single page **within** a section (its `subpath` changes) | ❌ No |

A fully stable identity that survives section renames and intra-section moves
would require an author-supplied front-matter `id` (or an engine-assigned durable
slug). That is intentionally out of scope today; `(sectionRef, subpath)` is the
cheap, high-value first step that covers the common case.

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
