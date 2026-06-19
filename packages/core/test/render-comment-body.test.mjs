import { test } from 'node:test'
import assert from 'node:assert/strict'
import { createRequire } from 'node:module'

const require = createRequire(import.meta.url)
const { renderCommentBody } = require('../index.js')

test('renders markdown emphasis to html', async () => {
  const html = await renderCommentBody('**bold**')
  assert.match(html, /<strong>bold<\/strong>/)
})

test('escapes raw html (never passes a <script> through)', async () => {
  const html = await renderCommentBody('<script>alert(1)</script>')
  assert.doesNotMatch(html, /<script>/)
})

test('blank input renders an empty string', async () => {
  assert.equal(await renderCommentBody('   '), '')
})
