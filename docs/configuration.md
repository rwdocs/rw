# Configuration

RW uses `rw.toml` for configuration. The file is automatically discovered in the current directory or any parent directory.

rw uses the nearest ancestor containing `rw.toml` as the project root, or the current directory if there is none; a command fails if that root has no docs directory (or `README.md` homepage); pass `--project-dir` to choose the root. (`rw confluence render` skips the docs check for a root you name, so it can render a file that belongs to no project.)

An explicitly supplied root is used as given and is **not** walked up from. Every command that reads a project (`rw serve`, `rw backstage publish`, `rw confluence render`, and each `rw comment` subcommand) takes `--config path/to/rw.toml` (short `-c`), which uses exactly that file, or `--project-dir path/to/project`, which roots the project at that directory, reading `path/to/project/rw.toml` if it exists and otherwise using defaults rooted there. `createSite({ projectDir })` in `@rwdocs/core` does the same. Walking up applies only to automatic discovery, so a project cannot silently inherit configuration from a parent directory it does not own.

`--project-dir` is what points `rw` at a project you are not in: it roots the whole project — the docs source directory, the `.rw/` data directory, the `README.md` homepage, and PlantUML include directories. To say where the markdown lives *within* a project, set `[docs] source_dir` in `rw.toml` instead. `--project-dir` and `--config` both name where the project is rooted, so they cannot be combined.

`source_dir` must be inside the project root, symlinks resolved. RW refuses to load a config whose `source_dir`, or the default `docs` folder, leads outside it: an absolute path elsewhere, `../docs`, or a symlink out. If the folder becomes a symlink out later, RW serves none of its pages. For docs that live above `rw.toml`, move `rw.toml` up to a folder that holds them and set `source_dir` from there.

## Full config example

```toml
[server]
host = "127.0.0.1"      # Server host
port = 7979              # Server port (see "Port selection" below)
allowed_hosts = []       # Extra host names to answer (see "Allowed hosts" below)

[docs]
source_dir = "docs"      # Markdown source directory, inside the project
cache_enabled = true     # Enable/disable caching (default: true)

[diagrams]
kroki_url = "https://kroki.io"  # Optional; when absent, diagram fences render as syntax-highlighted code, and diagram files show their alt text with a warning.
include_dirs = ["diagrams"]     # PlantUML !include search paths, inside the project

[live_reload]
enabled = true                  # Enable live reload (default: true)
```

## Environment Variables

String configuration values support environment variable expansion:

```toml
[diagrams]
kroki_url = "${KROKI_URL:-https://kroki.io}"
```

Supported syntax:

- `${VAR}` -- expands to the value of `VAR`, errors if unset
- `${VAR:-default}` -- expands to `VAR` if set, otherwise uses `default`
- `$${` -- a literal `${`

Any other `$`, including bare `$VAR`, is kept as written. Only the `$` directly
before `${` escapes it, so `$$${VAR}` gives `$${VAR}`. A `${` without a closing
`}` is an error.

Expandable fields: `server.host`, `diagrams.kroki_url`.

### `RW_DIAGRAMS_KROKI_URL` fallback

If no `rw.toml` (or no `[diagrams]` section) provides `diagrams.kroki_url`, the commands that render diagrams read `RW_DIAGRAMS_KROKI_URL` from the environment and use it. This lets a project render diagrams without a config file:

```bash
export RW_DIAGRAMS_KROKI_URL="https://kroki.internal"
cd path/to/repo-without-rw-toml
rw serve
```

Precedence (highest to lowest):

1. `--kroki-url` CLI flag
2. `[diagrams] kroki_url` in `rw.toml` (with `${VAR}` expansion if used)
3. `RW_DIAGRAMS_KROKI_URL` environment variable
4. (none) -- diagram code blocks render as plain text

An empty value (`RW_DIAGRAMS_KROKI_URL=`) is treated as unset.

Only `rw serve`, `rw confluence render`, and `rw comment add --quote` (which renders the page to find the quote) read the variable. Other commands ignore it, and so does `@rwdocs/core`: an embedding host passes `diagrams: { krokiUrl }` to `createSite`.

## CLI Overrides

CLI options override config file values:

```bash
# Use config file
rw serve

# Override port from config
rw serve --port 9000

# Use explicit config file
rw serve --config /path/to/rw.toml

# Serve a project you are not in
rw serve --project-dir /path/to/project
```

## Port selection

`rw serve` listens on port `7979` by default. If that port is already in use
(for example, another `rw serve` is running), it automatically falls back to the
next free port — `7980`, `7981`, and so on — and prints the port it settled on:

```
Port 7979 is in use, using 7980 instead
Starting server on http://127.0.0.1:7980
```

Fallback applies **only to the default port**. When you set a port explicitly —
either `--port` on the command line or `[server].port` in `rw.toml` — it is
treated as a hard requirement: if that port is busy, `rw serve` fails with an
error instead of quietly using a different one.

## Allowed hosts

`rw serve` answers requests addressed to `localhost`, a `*.localhost` name, or
an IP address, and refuses any other host name with 403. This stops a website
you visit from reaching your server through DNS rebinding.

To open the server by another name, such as `mylaptop.local` on your network
(with `host = "0.0.0.0"`) or a tunnel or cloud IDE address, allow it:

```toml
[server]
allowed_hosts = ["mylaptop.local", ".app.github.dev"]
```

or pass `--allowed-host mylaptop.local`, once per name. The flag adds to the
list in `rw.toml`. A name matches exactly, ignoring case. A leading `.` matches
the name and all its subdomains, so `.app.github.dev` allows
`app.github.dev` and `foo-7979.app.github.dev`. The entry `all` turns the check
off; use it only on a trusted network.

## README.md as Homepage

A directory's page is its `index.md`, or its `README.md` when it has no `index.md`, as in MkDocs and Docusaurus. So `docs/README.md` serves `/` and `docs/guides/README.md` serves `/guides`; no page is served at `/guides/README`. If `docs/` has neither file, RW uses `README.md` from the project root as the homepage. No configuration needed.

- A directory with both files is served from `index.md`. RW leaves its `README.md` out of the site and logs a warning naming it
- Links follow the file: a link to a `README.md` opens its directory's page when that README serves it, and is a broken link when the README isn't published
- Relative links resolve from the directory of the file they are in, as on GitHub. From `docs/guide.md`, `../README.md` is the project README, so it opens `/` only while that README is the homepage, and links in the project README resolve from the project root (`docs/guide.md` opens `/guide`). `../docs/x.md` from `docs/guide.md` opens `/x`, and so does `./docs/x.md` in the README while it is the homepage. Any other link out of `docs/` stays as written
- With a nested `source_dir`, such as `content/docs`, the same holds from where the files really are: `../../README.md` from `content/docs/guide.md` opens the homepage, and `content/docs/x.md` in the project README opens `/x`
- A link to a missing file inside `docs/` opens the URL its page would have, however the link spells it: `docs/missing.md` and `./docs/missing.md` in the project README both open `/missing`
- A link to a `.md` file inside `docs/` that serves no page, missing or a `README.md` left out, adds a page warning that `rw serve` logs: ``link `missing.md`: no page `missing` ``
- A `README.md` that is a symlink is the homepage only while its target is inside the project
- The directory's `meta.yaml` applies to its README page, with README frontmatter overriding it
- Live reload works for README.md changes too, including an `index.md` added or removed next to one
- The homepage appears as the first entry in the navigation sidebar
