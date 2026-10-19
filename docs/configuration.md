# Configuration

RW uses `rw.toml` for configuration. The file is automatically discovered in the current directory or any parent directory.

An explicitly supplied root is used as given and is **not** walked up from. `rw serve --config path/to/rw.toml` (short `-c`) uses exactly that file; `rw serve --project-dir path/to/project` and `rw backstage publish --project-dir path/to/project` root the project at that directory, reading `path/to/project/rw.toml` if it exists and otherwise using defaults rooted there; and `createSite({ projectDir })` in `@rwdocs/core` does the same. Walking up applies only to automatic discovery, so a project cannot silently inherit configuration from a parent directory it does not own.

`--project-dir` is what points `rw` at a project you are not in: it roots the whole project — the docs source directory, the `.rw/` data directory, the `README.md` homepage, and PlantUML include directories. To say where the markdown lives *within* a project, set `[docs] source_dir` in `rw.toml` instead. `--project-dir` and `--config` both name where the project is rooted, so they cannot be combined.

## Full config example

```toml
[server]
host = "127.0.0.1"      # Server host
port = 7979              # Server port (see "Port selection" below)

[docs]
source_dir = "docs"      # Markdown source directory
cache_enabled = true     # Enable/disable caching (default: true)

[diagrams]
kroki_url = "https://kroki.io"  # Optional; when absent, diagrams in markdown render as syntax-highlighted code (and `rw confluence render` emits a 'diagram skipped' warning).
include_dirs = ["."]            # PlantUML !include search paths

[live_reload]
enabled = true                  # Enable live reload (default: true)

[metadata]
name = "meta.yaml"              # Metadata file name (default: meta.yaml)
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

If no `rw.toml` (or no `[diagrams]` section) provides `diagrams.kroki_url`, RW reads `RW_DIAGRAMS_KROKI_URL` from the environment and uses it. This lets a project render diagrams without a config file:

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

## README.md as Homepage

A directory's page is its `index.md`, or its `README.md` when it has no `index.md`, as in MkDocs and Docusaurus. So `docs/README.md` serves `/` and `docs/guides/README.md` serves `/guides`; no page is served at `/guides/README`. If `docs/` has neither file, RW uses `README.md` from the project root as the homepage. No configuration needed.

- A directory with both files is served from `index.md`. RW leaves its `README.md` out of the site and logs a warning naming it (visible with `--verbose`)
- Links follow the file: a link to a `README.md` opens its directory's page when that README serves it, and is a broken link when the README isn't published
- Relative links resolve from the directory of the file they are in, as on GitHub. From `docs/guide.md`, `../README.md` is the project README, so it opens `/` only while that README is the homepage, and links in the project README resolve from the project root (`docs/guide.md` opens `/guide`). Any other link out of `docs/` stays as written
- The directory's `meta.yaml` applies to its README page, with README frontmatter overriding it
- Live reload works for README.md changes too, including an `index.md` added or removed next to one
- The homepage appears as the first entry in the navigation sidebar
