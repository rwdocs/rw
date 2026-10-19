# Tabs

Tabs show alternative versions of the same content, such as install steps
for each operating system, one at a time. A reader picks a tab by clicking it
or with the arrow, Home and End keys. The first tab is selected when the page
opens.

## Syntax

A `::::tabs` group wraps one or more `:::tab[Label]` items. Each item ends at
its own `:::`, and the group ends at `::::`:

````markdown
::::tabs

:::tab[macOS]

Install with Homebrew:

```sh
brew install rwdocs/tap/rw
```

:::

:::tab[Linux]

Install with the shell installer.

:::

::::
````

- `:::tab[Label]` — the bracketed text is the tab's label. A tab with no
  label is called `Tab`.
- A tab holds any markdown: paragraphs, lists, code blocks, diagrams, even
  another tab group.
- A closing line closes the innermost open group or tab, whatever its number
  of colons. Giving the group one more colon than its tabs keeps the nesting
  readable.

### Blank lines are required

Every `:::` line needs a blank line before and after it. Unlike
remark-directive and Docusaurus, rw does not recognize a directive line that
shares a paragraph with other lines, so this renders as one paragraph of text:

```markdown
::::tabs
:::tab[macOS]
Install with Homebrew.
:::
::::
```

rw warns when a paragraph like this holds a `::::tabs` or `:::tab` line, or a
closing `:::` inside a tab: `rw serve` logs the warning, and
`rw confluence render --strict` fails on it. Other `:::` directives, such as
`:::note`, are not ones rw knows, so they render as text with or without blank
lines and rw does not warn about them.

## Mistakes rw warns about

| Mistake | What renders |
|---|---|
| A `::::tabs` or `:::tab` line, or a closing `:::` inside a tab, without blank lines around it | The whole paragraph, as text |
| A missing closing `:::` or `::::` | Whatever is left open closes at the end of the page, or of the blockquote or list item it is in. Content after it stays inside the tab group; if the tab itself is never closed, the content lands in that tab's panel, hidden unless it is the first tab |
| A `:::tab` outside a `::::tabs` group | The tab's content, without tabs |
| A `::::tabs` group with no `:::tab` | The group's content, with an empty tab bar |
| A `:::` with nothing open to close | The colons, as text |

## Confluence publishing

Confluence storage format has no tabs macro, so `rw confluence render` shows
every tab at once: each tab's label as a bold paragraph, followed by its
content.
