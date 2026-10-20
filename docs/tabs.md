# Tabs

Tabs show alternative versions of the same content, such as install steps for
each operating system, one at a time. A reader picks a tab by clicking it or
with the arrow, Home and End keys. The first tab is selected when the page
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

- `:::tab[Label]`: the bracketed text is the tab's label. A tab with no label
  is called `Tab`.
- A tab holds any markdown: paragraphs, lists, code blocks, diagrams, even
  another tab group.
- A closing line closes the innermost open group or tab, whatever its number
  of colons. Giving the group one more colon than its tabs keeps the nesting
  readable.
- Blank lines around the `:::` lines are optional.

### Put the label in brackets

A directive line holds only the colons, the name, a `[label]` and `{attributes}`.
Anything else makes the whole line text: `:::tab macOS` is not a tab, and
neither is `:::tab[a]b]`. Write `:::tab[macOS]`. A label is plain text, so
`:::tab[**macOS**]` is text too.

### When a blank line is needed

Markdown reads some lines as part of the block above them. Leave a blank line:

- before a `:::` line that follows a list or a blockquote, or it becomes text
  inside the last list item or the quote;
- before a `:::` line that follows a table, or it becomes a table row;
- before a `:::` line that follows an HTML block, or it becomes part of the
  HTML;
- before a `---` or `===` line that follows a `:::` line, or the lines above it
  become a heading.

Inside a list item, the item must be loose (its paragraphs separated by blank
lines) and the tab lines indented to the item's text. In a tight item the
`:::` lines stay text. In all of these the lines show on the page as written,
and rw warns about the first of them in each block.

## Mistakes rw warns about

| Mistake | What renders |
|---|---|
| A missing closing `:::` or `::::` | Whatever is left open closes at the end of the page, or of the blockquote or list item it is in |
| A `:::tab` outside a `::::tabs` group | The tab's content, without tabs |
| A `::::tabs` group with no `:::tab` | The group's content, with an empty tab bar |
| A `:::` with nothing open to close, on a line of its own | The colons, as text |
| A directive line that needs a blank line (see [above](#when-a-blank-line-is-needed)) | The line, as text, or as part of a heading |
| Text after the name or label, such as `:::tab macOS`, or markup in a label | The line, as text |
| A directive name rw doesn't know, such as Docusaurus's `:::note` or `::youtube[id]` | The line, as text. rw warns once per name on each page |

`rw serve` logs these warnings, and `rw confluence render --strict` fails on
them.

## Confluence publishing

Confluence storage format has no tabs macro, so `rw confluence render` shows
every tab at once: each tab's label as a bold paragraph, followed by its
content.
