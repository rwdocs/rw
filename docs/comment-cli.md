# `rw comment` CLI

Read and write comments in a project's `.rw/comments/sqlite.db` from scripts and LLM agents (Claude Code, Cursor, etc.). The CLI opens the SQLite store directly — `rw serve` can be running or not; both processes can safely share the database (SQLite WAL).

## Subcommands

### `rw comment list`

List comment threads. Defaults to **open only**; client-side filter keeps top-level threads only (no replies) unless you pass `--parent`.

```
rw comment list [--document DOC] [--status open|resolved|all] [--parent ID|all]
```

Example:

```
rw comment list --document guide --status open
```

### `rw comment show`

Print a single thread — parent plus all replies — in text or JSON.

```
rw comment show <id>
```

### `rw comment add`

Create a new top-level comment. Page-level by default; pass `--quote "passage text"` to anchor it inline to a passage in the rendered page (see Anchoring below).

```
rw comment add --document DOC --body BODY [--quote "anchor text"]
```

Example:

```
rw comment add --document guide --quote "The quick brown fox" \
               --body "Can we add an example here?"
```

### `rw comment reply`

Reply to an existing thread. You only need the parent id — the CLI looks up the thread's document.

```
rw comment reply <parent-id> --body BODY
```

### `rw comment resolve`

Mark a thread resolved.

```
rw comment resolve <id>
```

## Identity

New comments carry an `author` stamp. Resolution order:

1. `--author-id` and `--author-name` flags on the subcommand.
2. `$RW_COMMENT_AUTHOR_ID` and `$RW_COMMENT_AUTHOR_NAME` env vars.
3. If both are unset, the CLI writes the comment with the default identity `{ id: "local:human", name: "You" }`. (This is the same default `rw serve` uses when the browser writes a comment — viewer and CLI stay aligned.)

Set both or neither — a partial identity (e.g. id without name) is rejected before the request leaves the CLI.

Recommended for LLM agents: export the env vars once (in `.claude/settings.json` or your shell profile) and let every `rw comment` invocation pick them up:

```
export RW_COMMENT_AUTHOR_ID=local:claude-code
export RW_COMMENT_AUTHOR_NAME="Claude Code"
```

## Anchoring with `--quote`

When you pass `--quote "..."` to `rw comment add`, the CLI:

1. Renders the referenced page.
2. Flattens the rendered HTML to a `textContent`-equivalent string (matches what the browser anchoring sees).
3. Searches for the quote as an **exact substring**.
4. Builds a `TextQuoteSelector` (with 32 chars of prefix/suffix context) plus a `TextPositionSelector`.

**Zero matches** → exit code 3, error `"quote not found in document 'X'"`. The quote must appear as rendered text — `**bold**` is not in the rendered output, `bold` is.

**Multiple matches** → exit code 3, error `"quote matches N times in document 'X' — add more surrounding context to disambiguate"`. Quote more surrounding text to pin the occurrence you want.

`--quote` and explicit selectors are mutually exclusive.

## Output format

`--format text` (default) is human-readable. `--format json` emits the raw API shape — for `show`, `{ "comment": Comment, "replies": [Comment, …] }`.

## Exit codes

| Code | Meaning |
|------|---------|
| 0    | Success |
| 1    | Internal error (store I/O, render failure, unexpected) |
| 2    | Comment not found (`show`, `resolve`, `reply <missing-parent>`) |
| 3    | Validation error (quote not found or ambiguous, document not found, invalid --parent uuid, mutually exclusive flags, partial identity) |

Shell example:

```
if ! rw comment add --document guide --quote "foo" --body "…"; then
  case $? in
    3) echo "bad quote, missing document, or invalid flags — see stderr" ;;
    *) echo "something went wrong — see stderr" ;;
  esac
fi
```
