# Status Badges

Status badges are inline colored pill labels — like `On Track` or `Blocked` —
for summarizing the state of something inline with prose. They mirror
Confluence's `status` macro, so pages published with `rw confluence update`
render as native, editable Confluence status badges.

## Syntax

```markdown
Billing is :status[On Track]{color=green}, search is :status[Behind]{color=yellow},
and the audit rewrite was :status[Declined]{color=grey}.
```

- `:status[Label]` — the bracketed text is the badge label.
- `{color=NAME}` — sets the badge color. Optional; defaults to `grey`.

## Colors

| Name     | Typical use            |
|----------|------------------------|
| `grey`   | Neutral, default       |
| `red`    | Blocked, failing       |
| `yellow` | At risk, in progress   |
| `green`  | On track, done         |
| `blue`   | Informational          |
| `purple` | Special status         |

Color names are case-insensitive. An unknown or omitted color falls back to
`grey` — no error.

## Confluence publishing

When published to Confluence, each badge becomes a native `status` structured
macro, so it stays editable in the Confluence editor and matches the
surrounding page style.
