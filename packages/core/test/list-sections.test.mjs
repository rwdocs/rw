import { test } from 'node:test'
import assert from 'node:assert/strict'
import { createRequire } from 'node:module'
import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'

const require = createRequire(import.meta.url)
const { createSite } = require('../index.js')

// Builds a temp project: root (no kind) -> billing (domain) -> payments
// (system) -> api (plain page). An rw.toml is required so `source_dir`
// resolves to `<root>/docs` rather than the process cwd.
function fixtureSite() {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'rw-core-sections-'))
  fs.writeFileSync(path.join(root, 'rw.toml'), '')
  const docs = path.join(root, 'docs')
  fs.mkdirSync(path.join(docs, 'billing', 'payments'), { recursive: true })
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
  return { site: createSite({ projectDir: root }), root }
}

test('listSections returns the full hierarchy flat with ancestry', async () => {
  const { site, root } = fixtureSite()
  try {
    const sections = await site.listSections()
    const byPath = Object.fromEntries(sections.map((s) => [s.path, s]))

    // root + billing + payments, sorted by path (root first).
    assert.deepEqual(
      sections.map((s) => s.path),
      ['', 'billing', 'billing/payments'],
    )

    assert.equal(byPath['billing'].sectionRef, 'domain:default/billing')
    assert.deepEqual(byPath['billing'].ancestors, ['section:default/root'])

    assert.equal(byPath['billing/payments'].sectionRef, 'system:default/payments')
    assert.deepEqual(byPath['billing/payments'].ancestors, [
      'domain:default/billing',
      'section:default/root',
    ])

    // The root section has no ancestors.
    assert.equal(byPath[''].sectionRef, 'section:default/root')
    assert.deepEqual(byPath[''].ancestors, [])
  } finally {
    fs.rmSync(root, { recursive: true, force: true })
  }
})
