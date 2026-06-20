# Embedding & durable comment keys

When you embed `@rwdocs/viewer` (or render pages through `@rwdocs/core`) in a host
application that stores **its own** comments — for example the Backstage plugin
pair — each comment needs a stable identifier for the page it annotates.

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
