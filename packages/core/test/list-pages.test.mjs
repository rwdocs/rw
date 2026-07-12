import { test } from 'node:test'
import assert from 'node:assert/strict'
import { createRequire } from 'node:module'
import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'

const require = createRequire(import.meta.url)
const { createSite } = require('../index.js')

// root (no kind) -> billing (domain) -> payments (system) -> api (plain page).
// `archive/` declares a meta.yaml but has no index.md, which is what makes it a
// virtual page — a page with a title and a place in the tree but no body, i.e.
// hasContent: false. An rw.toml is required so `source_dir` resolves to
// `<root>/docs`, not the process cwd.
function fixtureSite() {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'rw-core-pages-'))
  fs.writeFileSync(path.join(root, 'rw.toml'), '')
  const docs = path.join(root, 'docs')
  fs.mkdirSync(path.join(docs, 'billing', 'payments'), { recursive: true })
  fs.mkdirSync(path.join(docs, 'archive'), { recursive: true })
  fs.writeFileSync(path.join(docs, 'index.md'), '# Home\n')
  fs.writeFileSync(
    path.join(docs, 'billing', 'index.md'),
    '---\nkind: domain\n---\n# Billing\n',
  )
  fs.writeFileSync(
    path.join(docs, 'billing', 'payments', 'index.md'),
    '---\nkind: system\n---\n# Payments\n',
  )
  fs.writeFileSync(path.join(docs, 'billing', 'payments', 'api.md'), '# API\n')
  fs.writeFileSync(path.join(docs, 'archive', 'meta.yaml'), 'title: Archive\n')
  fs.writeFileSync(path.join(docs, 'archive', 'note.md'), '# Note\n')
  return { site: createSite({ projectDir: root }), root }
}

// Every page the fixture produces. Pinned so a regression that silently drops a
// page fails here, rather than passing a lookup-what-you-expect check.
const ALL_PATHS = [
  '',
  'archive',
  'archive/note',
  'billing',
  'billing/payments',
  'billing/payments/api',
]

test('listPages carries each page site path, anchors, and hasContent', async () => {
  const { site, root } = fixtureSite()
  try {
    const pages = await site.listPages()
    const byPath = Object.fromEntries(pages.map((p) => [p.path, p]))

    // Every page is listed, virtual ones included.
    assert.deepEqual(pages.map((p) => p.path).sort(), ALL_PATHS)

    // A page nested two sections deep: anchors run innermost-first, root last,
    // each subpath relative to that anchor's own section.
    const api = byPath['billing/payments/api']
    assert.equal(api.title, 'API')
    assert.equal(api.sectionRef, 'system:default/payments')
    assert.equal(api.subpath, 'api')
    assert.deepEqual(api.anchors, [
      { sectionRef: 'system:default/payments', subpath: 'api' },
      { sectionRef: 'domain:default/billing', subpath: 'payments/api' },
      { sectionRef: 'section:default/root', subpath: 'billing/payments/api' },
    ])

    // anchors[0] IS the page's (sectionRef, subpath) identity; anchors[last] is
    // the root section, whose subpath is the site path.
    for (const page of pages) {
      assert.equal(page.anchors[0].sectionRef, page.sectionRef)
      assert.equal(page.anchors[0].subpath, page.subpath)
      assert.equal(page.anchors.at(-1).sectionRef, 'section:default/root')
      assert.equal(page.anchors.at(-1).subpath, page.path)
    }

    // The root page: empty path, single root anchor.
    assert.deepEqual(byPath[''].anchors, [
      { sectionRef: 'section:default/root', subpath: '' },
    ])

    // archive/ has no index.md, so it is a virtual page with no body...
    assert.equal(byPath['archive'].hasContent, false)
    // ...while every real markdown page has one.
    assert.equal(byPath['archive/note'].hasContent, true)
    assert.equal(api.hasContent, true)
  } finally {
    fs.rmSync(root, { recursive: true, force: true })
  }
})

// The point of `path`: it is the key the read methods take, so a host goes
// straight from a listing to a read with no path arithmetic — and it is exactly
// what pagePathFor() would have returned for the same page's identity.
test('listPages path reads the right page, and matches pagePathFor', async () => {
  const { site, root } = fixtureSite()
  try {
    const pages = await site.listPages()
    // Pin the set first, so the loop below can never pass by running zero times.
    assert.deepEqual(pages.map((p) => p.path).sort(), ALL_PATHS)

    for (const page of pages) {
      // `path` is the identity's resolution — the round trip a host no longer
      // has to make.
      assert.equal(await site.pagePathFor(page.sectionRef, page.subpath), page.path)

      // Prove `path` leads to the RIGHT page, not merely a readable one: a
      // mis-join landing on some other existing page would pass a bare "it
      // didn't reject" check. The page's own title is the oracle. A virtual
      // page has no markdown, which is exactly what hasContent advertises.
      const markdown = await site.getPageMarkdown(page.path)
      if (page.hasContent) {
        assert.match(markdown.markdown, new RegExp(`^# ${page.title}$`, 'm'))
      } else {
        assert.equal(markdown, null)
      }
    }
  } finally {
    fs.rmSync(root, { recursive: true, force: true })
  }
})
