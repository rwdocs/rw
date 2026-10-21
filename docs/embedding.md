# Embedding

This page covers integration concerns for embedding `@rwdocs/viewer` (or rendering pages
through `@rwdocs/core`) in a host application: pointing `@rwdocs/core` at a site
with `projectDir`, cleaning up the S3 render cache, publishing from CI, serving page images,
reading page attrs, choosing durable comment keys for hosts that store their own comments,
sanitizing comment HTML you supply, passing paths to the viewer, resolving
cross-entity links via `resolveSectionRefs`, and reading section ancestry.

To see your docs embedded before wiring up a host, run `rw serve` and open
`/_embedded/`: a Backstage-like host page that mounts the viewer with
`basePath: "/_embedded"`.

## `projectDir` roots every path

`createSite({ projectDir })` in `@rwdocs/core` names the project root, and every
path RW uses is derived from it: the docs source directory and the PlantUML
`!include` search directories. If `projectDir` contains an `rw.toml`, that file
is loaded and its relative paths resolve against `projectDir`; if it does not,
defaults rooted at `projectDir` are used.

`createSite` does not read `RW_DIAGRAMS_KROKI_URL`. A `projectDir` site renders
diagrams only when its `rw.toml` sets `[diagrams] kroki_url` or the host passes
`diagrams: { krokiUrl }`, which takes precedence.

A `projectDir` site has no render cache: every page is rendered on request, and
`cache_enabled` in `rw.toml` has no effect. Nothing is written under
`projectDir`. The `.rw/` directory where `rw serve` keeps its cache and comments
database is neither read nor created.

RW does **not** walk up from `projectDir` looking for an `rw.toml` in a parent
directory, and it does not consult the Node process's working directory. A host
that mounts several sites can therefore point each `createSite` call at its own
directory without one site's configuration leaking into another.

## The S3 render cache is per rw version

`createSite({ s3 })` caches rendered pages and diagrams next to the entity's
bundle, under `cache/<rw version>/` (for example
`<bucketRootPath>/<entity>/cache/0.1.37/pages/`). A new rw version starts from an
empty cache, so rendering fixes reach every page without anyone clearing it.

Nothing deletes the caches of earlier versions. Remove them by hand or with a
bucket lifecycle rule: every `cache/<version>/` other than the current one, and
the unversioned `cache/pages/` and `cache/diagrams/` left by older RW versions.

## Publish from CI with full history

`rw backstage publish` records each page's last-modified time in the bundle
manifest: the time of the last commit that changed the page. Finding that commit
needs history, so check out all of it:

```yaml
- uses: actions/checkout@v7
  with:
    fetch-depth: 0
- run: curl --proto '=https' --tlsv1.2 -LsSf https://github.com/rwdocs/rw/releases/latest/download/rw-installer.sh | sh
- run: rw backstage publish --entity default/Component/arch --bucket my-docs
  env:
    AWS_ACCESS_KEY_ID: ${{ secrets.AWS_ACCESS_KEY_ID }}
    AWS_SECRET_ACCESS_KEY: ${{ secrets.AWS_SECRET_ACCESS_KEY }}
```

With the default `fetch-depth: 1`, every page gets the time of the commit being
built, so each push, even one that touches only code, changes every page's
`lastModified` and makes every Backstage reader rescan the site.

To skip images it uploaded before, `rw backstage publish` lists the entity's
`files/`, which needs `s3:ListBucket`; without it, rw uploads every image each
time. An image keeps its first upload time for as long as pages show it, so
don't expire `files/` by age with a lifecycle rule.

## Serve page images with `getFile`

A page can show [image files stored in the project](images.md).
`renderPage()` writes each one's `src` as `/_api/files/{path}`, the `rw serve`
route, where `path` is the file's path in the project, percent-encoded:
`docs/img/flow.png`. Before showing the page, the viewer moves that prefix
under the `apiBaseUrl` you pass to `mountRw`: with `apiBaseUrl: "/api/docs"`,
the browser requests `/api/docs/files/docs/img/flow.png` and never the
`/_api/files` URL. If you show pages without the viewer, rewrite the prefix
yourself.

Serve `GET {apiBaseUrl}/files/{*path}` beside your page and navigation routes.
Pass the rest of the URL path, percent-decoded, to `site.getFile(path)`, and
don't resolve `..` or check it yourself: `getFile` refuses every path outside
the project or off the image allowlist.

- `null` means not found or refused. Answer 404 without saying which.
- Otherwise answer 200 with `content` as the body and these headers:
  - `Content-Type`: `contentType`
  - `ETag`: `etag`, a strong tag already in quotes
  - `Cache-Control: no-cache`, so the browser revalidates and an image
    republished under the same path shows on the next load
  - `X-Content-Type-Options: nosniff`
  - `Content-Security-Policy: sandbox`, so an SVG opened on its own can't run
    script
- When `If-None-Match` lists `etag` or `*`, answer 304 with `ETag`,
  `Cache-Control` and `Content-Security-Policy: sandbox`. Compare weakly:
  ignore a `W/` prefix on a listed tag. Express's `req.fresh` does this once
  the `ETag` header is set. A browser replaces a cached response's headers
  with the 304's, and Backstage adds its own `Content-Security-Policy` to
  every response, so a 304 without `sandbox` unsandboxes the cached file.
- `getFile` rejects when storage fails, for example when S3 is unreachable.
  Let that surface as a 5xx, not a 404.

An `<img>` request carries the user's cookies, not the Bearer token the
viewer's `fetchFn` adds. In Backstage, allow the route with a `user-cookie` auth
policy, as TechDocs does for its static files, and pass
`allowLimitedAccess: true` to the `httpAuth` calls in its handler, or every
`<img>` request gets a 401. The backend's `/.backstage/auth/v1/cookie`
endpoint issues the cookie; the frontend has to request and refresh it, for
example with `CookieAuthRefreshProvider`.

## Consuming page attrs

Only `renderPage()` in `@rwdocs/core` exposes resolved [page-local `attrs`](metadata.md#attrs-page-local-integration-data):

```js
const page = await site.renderPage("guide");
const owner = page.meta.attrs?.owner;
if (typeof owner === "string") {
  // Validate against your integration's schema before using it.
}
```

`meta.attrs` is omitted when the page has none and is typed as
`Record<string, JsonValue>`. Every key, including `__proto__`, is an ordinary
own property, so copy attrs with spread, not `Object.assign`, which treats
`__proto__` as the prototype setter. RW does not interpret attrs, so validate
them against your own schema and do not treat them as executable configuration
or authorization rules. Use strings for identifiers that need exact large integers.

`meta.attrs` is as fresh as the rest of the page metadata, even when the page
HTML comes from the render cache.

## Don't key on `path`

When you embed the viewer in a host application that stores **its own**
comments — for example the Backstage plugin pair — each comment needs a stable
identifier for the page it annotates.

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
- **`documentId`** — the two joined by `#`, e.g. `domain:default/billing#api`.
  The viewer passes it as `documentId` to your `CommentApiClient`, and
  `rw serve` stores its own comments under it.

The pair `(sectionRef, subpath)` is your durable comment key. It **survives a
whole-section remount**: the URL prefix changes, but each page's `subpath`
(relative to its unchanged section) does not.

## What it does and does not survive

| Change | `(sectionRef, subpath)` survives? |
|--------|-----------------------------------|
| Mounting a whole section under a different base URL, its `sectionRef` unchanged | ✅ Yes |
| Relocating a whole section's directory while its effective name, kind, and namespace stay the same | ✅ Yes |
| Renaming a section directory while keeping explicit `name`, kind, and namespace | ✅ Yes |
| Changing a section's effective name (its `sectionRef` changes) | ❌ No |
| Moving a single page **within** a section (its `subpath` changes) | ❌ No |

To rename a section directory without changing keys, first declare its current
derived name as [metadata `name`](metadata.md#name-identity-independent-of-the-documentation-path).
RW does not migrate comments when a key changes.

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
  plugin does this.
- **Omit `bodyHtml`** and return only the plain-text `body` — the viewer renders
  it as text, with no HTML injection.

Returning unsanitized HTML — or proxying `bodyHtml` straight from an upstream
store — lets comment authors inject scripts that execute in your page's origin
(stored XSS).

## Hand the viewer your location as the browser shows it

`mountRw` takes two strings that place it in your URLs:

- **`basePath`** — the path where your app shows `sectionRef`'s root page,
  URL-encoded as `location.pathname` shows it, such as
  `/catalog/default/system/payment-gateway/docs`. Pass `""` when the section's
  root is your origin's `/`. A trailing slash is ignored.
- **`location`** — the page to open: `location.pathname + location.search +
  location.hash`, exactly as the browser shows it, still URL-encoded. If it
  doesn't start with `basePath`, the viewer opens the section's root page.

`navigateTo(location)` takes the same form. Call it on every location change,
including Back/Forward and changes to the hash alone; it ignores a location
outside `basePath`. Don't decode the location or strip `basePath`
yourself: the viewer does both, and a decoded path can't tell a page named
`c#` from page `c` with a hash.

`onNavigate` hands you the same form for navigation the viewer starts, such as
a link click, and is never called for your own `navigateTo` calls, so it can
push a history entry every time.

```js
const hostLocation = () => location.pathname + location.search + location.hash;

const rw = mountRw(root, {
  sectionRef,
  basePath: "/catalog/default/system/payment-gateway/docs",
  location: hostLocation(),
  onNavigate: (href) => history.pushState({}, "", href),
  /* ... */
});
addEventListener("popstate", () => rw.navigateTo(hostLocation()));
```

Don't add the section's directory path (for example `/systems/payment-gateway`)
yourself; the viewer adds it.

### Migrating from `initialPath`

Up to 0.1.36, `mountRw` took `initialPath` and `navigateTo` a path with
`basePath` already stripped, and learned its base URL by passing its own
`sectionRef` to `resolveSectionRefs`. Now:

- Replace `initialPath: viewerPath()` with `basePath` plus `location`, and pass
  the full location to `navigateTo` too.
- Call `navigateTo` on hash-only changes as well. The viewer no longer listens
  for `popstate` itself.
- Update your URL yourself when you call `navigateTo`; `onNavigate` no longer
  reports it.
- Drop `embedded`; `mountRw` always embeds.

`resolveSectionRefs` still maps your own `sectionRef` for links and navigation
that point at it; only the base URL comes from `basePath` now.

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
last resort for a host that violates the contract, not something to rely on
for correct cross-entity navigation.

## Resolve section ancestry with `sectionAncestry`

Both the page response (`renderPage` / the page HTTP API) and the navigation
response (`getNavigation` / the navigation HTTP API) include a `sectionAncestry`
map, so a host can resolve a page's or view's full section context from a single
response instead of walking sections with follow-up calls.

- **Keys** are section refs (`kind:namespace/name`).
- **Values** are ancestry chains — arrays of `{ sectionRef, subpath }` anchors.
  Each chain **starts with the section itself** (empty `subpath`), then its
  ancestors nearest-first with the root section last.
- A **`subpath`** here, as in `PageMeta`, is a page path, not a URL: a page
  named `c#` has subpath `c#`. Encode each segment before putting one in a URL.

A page's map covers the page's own section, every section it links to, and its
breadcrumb sections; a navigation view's map covers its items, scope, and parent
scope. Look up a page's own section by its `sectionRef`:

```js
// The ancestry chain of the page's own section, root last.
const chain = page.sectionAncestry[page.meta.sectionRef];
```

The map always has the page's own section and a view's scope, so that lookup
never misses. When two sections share a ref, the map holds the chain of the one
that [ref lookup chooses](metadata.md#name-identity-independent-of-the-documentation-path),
even for a page under the other one.
