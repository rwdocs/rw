import { test } from 'node:test'
import assert from 'node:assert/strict'
import { createRequire } from 'node:module'
import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'

const require = createRequire(import.meta.url)
const { createSite } = require('../index.js')

// Builds a temp project: root (no kind) -> billing (domain) -> payments
// (system), with pages inside each. `archive/` declares a meta.yaml but has no
// index.md, which is what makes it a virtual page — an identity listPages()
// emits but getPageMarkdown() reads as null. An rw.toml is required so
// `source_dir` resolves to `<root>/docs` rather than the process cwd.
function fixtureSite() {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'rw-core-page-path-'))
  fs.writeFileSync(path.join(root, 'rw.toml'), '')
  const docs = path.join(root, 'docs')
  fs.mkdirSync(path.join(docs, 'billing', 'payments'), { recursive: true })
  fs.mkdirSync(path.join(docs, 'archive'), { recursive: true })
  fs.writeFileSync(path.join(docs, 'index.md'), '# Home\n')
  fs.writeFileSync(path.join(docs, 'guide.md'), '# Guide\n')
  fs.writeFileSync(
    path.join(docs, 'billing', 'index.md'),
    '---\nkind: domain\n---\n# Billing\n',
  )
  fs.writeFileSync(
    path.join(docs, 'billing', 'payments', 'index.md'),
    '---\nkind: system\n---\n# Payments\n',
  )
  fs.writeFileSync(
    path.join(docs, 'billing', 'payments', 'api.md'),
    '# API\n\nBody.\n',
  )
  fs.writeFileSync(path.join(docs, 'archive', 'meta.yaml'), 'title: Archive\n')
  fs.writeFileSync(path.join(docs, 'archive', 'note.md'), '# Note\n')
  return { site: createSite({ projectDir: root }), root }
}

// Every page the fixture produces, as pagePathFor should resolve it. Pinned so
// the round-trip below asserts an exact inverse mapping rather than "nothing
// threw" — a listPages() that dropped pages would otherwise still pass.
const ALL_PATHS = [
  '',
  'archive',
  'archive/note',
  'billing',
  'billing/payments',
  'billing/payments/api',
  'guide',
]

// The virtual page: it has an identity, but no markdown of its own.
const VIRTUAL_PATH = 'archive'

async function withSite(fn) {
  const { site, root } = fixtureSite()
  try {
    await fn(site)
  } finally {
    fs.rmSync(root, { recursive: true, force: true })
  }
}

// The point of the method: every identity listPages() hands out must resolve
// back to a path the read methods accept. This is the round-trip a host does.
test('every listPages identity resolves to a readable path', async () => {
  await withSite(async (site) => {
    const pages = await site.listPages()
    const resolved = []

    for (const page of pages) {
      const path = await site.pagePathFor(page.sectionRef, page.subpath)
      assert.notEqual(
        path,
        null,
        `${page.sectionRef}#${page.subpath} did not resolve`,
      )
      resolved.push(path)

      // Prove the path leads to the RIGHT page, not merely a readable one: a
      // mis-join that lands on some other existing page would pass a bare
      // "it didn't reject" check. The page's own title is the oracle.
      // (getPageMarkdown resolves to null for a virtual page rather than
      // rejecting, so that class needs its own assertion.)
      const markdown = await site.getPageMarkdown(path)
      if (path === VIRTUAL_PATH) {
        assert.equal(markdown, null)
      } else {
        assert.match(markdown.markdown, new RegExp(`^# ${page.title}$`, 'm'))
      }
    }

    assert.deepEqual(resolved.sort(), ALL_PATHS)
  })
})

test('resolves a page nested inside a section', async () => {
  await withSite(async (site) => {
    assert.equal(
      await site.pagePathFor('system:default/payments', 'api'),
      'billing/payments/api',
    )
  })
})

test("resolves a section's root page", async () => {
  await withSite(async (site) => {
    assert.equal(await site.pagePathFor('domain:default/billing', ''), 'billing')
  })
})

// A page in no explicit section keys on the implicit root ref.
test('resolves a page outside any explicit section', async () => {
  await withSite(async (site) => {
    assert.equal(await site.pagePathFor('section:default/root', 'guide'), 'guide')
  })
})

// The sharp edge: the site's root page is the EMPTY STRING, not null. A caller
// testing `if (!path)` would 404 its own homepage.
test('resolves the site root page to an empty string, not null', async () => {
  await withSite(async (site) => {
    const resolved = await site.pagePathFor('section:default/root', '')
    assert.equal(resolved, '')

    const page = await site.getPageMarkdown(resolved)
    assert.equal(page.markdown, '# Home\n')
  })
})

// Refs carry a namespace (kind:namespace/name) and the reverse index keys on
// the whole thing, so a section outside the default namespace must round-trip
// too — nothing else in this file leaves `default`.
test('resolves a page in a non-default namespace', async () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'rw-core-page-path-ns-'))
  try {
    fs.writeFileSync(path.join(root, 'rw.toml'), '')
    const docs = path.join(root, 'docs')
    fs.mkdirSync(path.join(docs, 'ledger'), { recursive: true })
    fs.writeFileSync(path.join(docs, 'index.md'), '# Home\n')
    fs.writeFileSync(
      path.join(docs, 'ledger', 'index.md'),
      '---\nkind: system\nnamespace: payments\n---\n# Ledger\n',
    )
    fs.writeFileSync(path.join(docs, 'ledger', 'api.md'), '# API\n')
    const site = createSite({ projectDir: root })

    assert.equal(
      await site.pagePathFor('system:payments/ledger', 'api'),
      'ledger/api',
    )
    // The same name in the default namespace is a different section entirely.
    assert.equal(await site.pagePathFor('system:default/ledger', 'api'), null)
  } finally {
    fs.rmSync(root, { recursive: true, force: true })
  }
})

test('resolves to null for an unknown section ref', async () => {
  await withSite(async (site) => {
    assert.equal(await site.pagePathFor('domain:default/nope', 'api'), null)
  })
})

// A host can hand over a stale or hand-built ref. None of these may throw, and
// none may fall through to the implicit root — whose scope is the empty string,
// the one entry a sloppy match could collide with.
test('resolves to null for a malformed section ref', async () => {
  await withSite(async (site) => {
    for (const ref of ['', 'garbage', 'domain:billing', 'domain:default/']) {
      assert.equal(await site.pagePathFor(ref, 'api'), null, `ref: ${ref}`)
    }
  })
})

// Not a page-existence check: it builds the requested path and the read that
// follows is what rejects.
test('resolves a subpath that names no page, leaving the read to reject', async () => {
  await withSite(async (site) => {
    const resolved = await site.pagePathFor('domain:default/billing', 'ghost')

    assert.equal(resolved, 'billing/ghost')
    await assert.rejects(() => site.getPageMarkdown(resolved), /not found/i)
  })
})
