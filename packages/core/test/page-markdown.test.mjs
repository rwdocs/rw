import { test } from 'node:test'
import assert from 'node:assert/strict'
import { createRequire } from 'node:module'
import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'

const require = createRequire(import.meta.url)
const { createSite } = require('../index.js')

// A root page, a page with frontmatter, and `section/`, which declares a
// meta.yaml but has no index.md — that is what makes it a virtual page. (A
// directory with neither gets no page of its own; its children hoist to the
// parent.) An rw.toml is required so `source_dir` resolves to `<root>/docs`
// rather than the process cwd.
function fixtureSite() {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'rw-core-markdown-'))
  fs.writeFileSync(path.join(root, 'rw.toml'), '')
  const docs = path.join(root, 'docs')
  fs.mkdirSync(path.join(docs, 'section'), { recursive: true })
  fs.writeFileSync(path.join(docs, 'index.md'), '# Home\n')
  fs.writeFileSync(path.join(docs, 'guide.md'), GUIDE_SOURCE)
  fs.writeFileSync(path.join(docs, 'section', 'meta.yaml'), 'title: Section\n')
  fs.writeFileSync(path.join(docs, 'section', 'child.md'), CHILD_SOURCE)
  return createSite({ projectDir: root })
}

// Frontmatter is part of the page, not noise to strip: it carries the title
// and kind an agent can use.
const GUIDE_SOURCE = [
  '---',
  'title: Guide',
  'kind: domain',
  '---',
  '',
  '# Guide',
  '',
  'Body.',
  '',
].join('\n')

// Deliberately full of things the HTML renderer would transform: a wikilink, a
// directive and a diagram fence. getPageMarkdown must hand all three back
// untouched.
const CHILD_SOURCE = [
  '# Child',
  '',
  'See [[billing::api]] and :status[Done]{color=green}.',
  '',
  '```plantuml',
  'A -> B',
  '```',
  '',
].join('\n')

test('getPageMarkdown keeps the YAML frontmatter', async () => {
  const site = fixtureSite()
  const page = await site.getPageMarkdown('guide')
  assert.equal(page.markdown, GUIDE_SOURCE)
})

test('getPageMarkdown reads the site root page', async () => {
  const site = fixtureSite()
  const page = await site.getPageMarkdown('')
  assert.equal(page.markdown, '# Home\n')
})

test('getPageMarkdown reads a README.md homepage', async () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'rw-core-readme-'))
  fs.writeFileSync(path.join(root, 'rw.toml'), '')
  fs.writeFileSync(path.join(root, 'README.md'), '# Readme Home\n\nBody.\n')
  const site = createSite({ projectDir: root })

  const page = await site.getPageMarkdown('')

  assert.equal(page.markdown, '# Readme Home\n\nBody.\n')
})

test('getPageMarkdown leaves wikilinks, directives and diagram fences untouched', async () => {
  const site = fixtureSite()
  const page = await site.getPageMarkdown('section/child')
  assert.equal(page.markdown, CHILD_SOURCE)
})

test('getPageMarkdown returns null for a virtual directory page', async () => {
  const site = fixtureSite()
  assert.equal(await site.getPageMarkdown('section'), null)
})

test('getPageMarkdown rejects for an unknown page', async () => {
  const site = fixtureSite()
  await assert.rejects(() => site.getPageMarkdown('nope'), /not found/i)
})
