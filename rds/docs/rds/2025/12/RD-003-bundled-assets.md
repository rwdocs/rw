# RD-003: Bundled Frontend Assets

## Overview

Bundle the compiled frontend assets (JavaScript, CSS, HTML) into the `docstage` Python
package. The server always uses bundled assets with no CLI override option.

## Problem Statement

Currently, the `docstage serve` command requires a `--static-dir` option pointing to the
frontend build output:

```bash
uv run docstage serve --static-dir frontend/dist
```

This creates deployment friction:

1. Users must manually build the frontend before running the server.
2. The `--static-dir` path must be correctly specified.
3. Frontend and backend versions can become mismatched.
4. Distribution requires coordinating multiple artifacts.

## Goals

1. Bundle frontend assets into the `docstage` Python package.
2. Serve bundled assets automatically—no CLI options needed.
3. Simplify CLI by removing `--static-dir` option entirely.

## Non-Goals

- Server-side rendering (SSR).
- CDN distribution of assets.
- Frontend asset versioning independent of package version.
- Custom static directory override (use Vite dev server for development).

## Architecture

### Package Structure

```
packages/docstage/
├── pyproject.toml
└── src/docstage/
    ├── __init__.py
    ├── cli.py
    ├── server.py
    ├── static/                 # Bundled frontend assets
    │   ├── index.html
    │   ├── favicon.png
    │   └── assets/
    │       ├── index-*.css
    │       └── index-*.js
    └── ...
```

### Request Flow

```
┌─────────────────────────────────────────────────────────────────────────────┐
│  HTTP Request                                                               │
│                                                                             │
│  Path starts with /api/?                                                    │
│         │                                                                   │
│         ├─ Yes ─→ Route to API handlers                                     │
│         │                                                                   │
│         └─ No ──→ Serve from bundled static assets                          │
│                   ├─ /assets/* → static files                               │
│                   ├─ /favicon.png → favicon                                 │
│                   └─ /* → index.html (SPA fallback)                         │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Implementation

### Phase 1: Package Configuration

Modify `pyproject.toml` to include static assets in the wheel:

```toml
[tool.hatch.build.targets.wheel]
packages = ["src/docstage"]

[tool.hatch.build.targets.wheel.force-include]
"../../frontend/dist" = "src/docstage/static"
```

This copies the frontend build output into the package during wheel creation.

### Phase 2: Asset Discovery

Add a module to locate bundled assets at runtime:

```python
# src/docstage/assets.py
from importlib.resources import files
from pathlib import Path


def get_static_dir() -> Path:
    """Return path to bundled static assets.

    Raises:
        FileNotFoundError: If static assets are not bundled.
    """
    static = files("docstage").joinpath("static")
    if not static.is_dir():
        msg = (
            "Bundled static assets not found. "
            "Run 'cd frontend && npm run build' then 'uv sync --reinstall'."
        )
        raise FileNotFoundError(msg)
    # Convert Traversable to Path for aiohttp compatibility
    return Path(str(static))
```

### Phase 3: Server Integration

Update `server.py` to always use bundled assets:

```python
from docstage.assets import get_static_dir

class ServerConfig(TypedDict):
    """Server configuration."""
    host: str
    port: int
    source_dir: Path
    cache_dir: Path
    # static_dir removed - always use bundled assets


def create_app(config: ServerConfig) -> web.Application:
    # ...

    static_dir = get_static_dir()
    app["static_dir"] = static_dir

    assets_dir = static_dir / "assets"
    if assets_dir.exists():
        app.router.add_static("/assets", assets_dir)

    app.router.add_get("/favicon.png", _serve_favicon)
    app.router.add_get("/{path:.*}", spa_fallback)
```

### Phase 4: CLI Cleanup

Remove `--static-dir` option from the `serve` command:

```python
@cli.command()
@click.option(
    "--source-dir", "-s",
    type=click.Path(exists=True, path_type=Path, file_okay=False),
    default="docs",
    help="Documentation source directory",
)
@click.option(
    "--cache-dir",
    type=click.Path(path_type=Path, file_okay=False),
    default=".cache",
    help="Cache directory",
)
@click.option("--host", "-h", default="127.0.0.1", help="Host to bind to")
@click.option("--port", "-p", type=int, default=8080, help="Port to bind to")
def serve(source_dir: Path, cache_dir: Path, host: str, port: int) -> None:
    """Start the documentation server."""
    config: ServerConfig = {
        "host": host,
        "port": port,
        "source_dir": source_dir.resolve(),
        "cache_dir": cache_dir.resolve(),
    }
    # ...
```

### Phase 5: Build Script

Add `build:bundle` script to `frontend/package.json`:

```json
{
  "scripts": {
    "build:bundle": "vite build && rm -rf ../packages/docstage/src/docstage/static && cp -r dist ../packages/docstage/src/docstage/static"
  }
}
```

### Phase 6: Gitignore Configuration

Add bundled assets to `.gitignore` since they are build artifacts:

```gitignore
# Bundled frontend assets (generated from frontend/dist)
packages/docstage/src/docstage/static/
```

### Phase 7: Test Updates

Remove tests for `--static-dir` option and update remaining tests:

```python
# tests/test_server.py
async def test_serves_bundled_index_html(aiohttp_client, tmp_path):
    """Root path serves bundled index.html."""
    config: ServerConfig = {
        "host": "127.0.0.1",
        "port": 8080,
        "source_dir": tmp_path / "docs",
        "cache_dir": tmp_path / "cache",
    }
    app = create_app(config)
    client = await aiohttp_client(app)

    response = await client.get("/")
    assert response.status == 200
    assert "text/html" in response.headers["Content-Type"]
```

## Development Workflow

### Frontend Development

Use Vite dev server with API proxy for hot module replacement:

```bash
# Terminal 1: Backend API server
uv run docstage serve --source-dir docs

# Terminal 2: Frontend dev server with HMR
cd frontend
npm run dev
```

Access the app at `http://localhost:5173` (Vite dev server). API requests are proxied
to the backend at `http://localhost:8080`.

### Production Build

```bash
# Build frontend and bundle into backend
cd frontend
npm run build:bundle

# Run server (uses bundled assets)
cd ..
uv run docstage serve --source-dir docs
```

## Testing

### Unit Tests

```python
# tests/test_assets.py
import pytest
from docstage.assets import get_static_dir


def test_get_static_dir_returns_path():
    """Bundled assets directory should be accessible."""
    static_dir = get_static_dir()
    assert static_dir.exists()
    assert (static_dir / "index.html").exists()


def test_get_static_dir_raises_when_missing(monkeypatch):
    """Should raise FileNotFoundError when static directory doesn't exist."""
    from importlib.resources import files

    class FakeTraversable:
        def is_dir(self):
            return False

        def joinpath(self, name):
            return self

    monkeypatch.setattr("docstage.assets.files", lambda _: FakeTraversable())

    with pytest.raises(FileNotFoundError, match="Bundled static assets not found"):
        get_static_dir()
```

## Migration

### Breaking Changes

- The `--static-dir` CLI option is removed.
- `ServerConfig` no longer includes `static_dir` field.

### For Users

Update any scripts or configurations that use `--static-dir`:

```bash
# Before
uv run docstage serve --static-dir frontend/dist --source-dir docs

# After
uv run docstage serve --source-dir docs
```

## Dependencies

No new dependencies required. Uses:

- `importlib.resources` (standard library, Python 3.9+)
- `hatchling` build backend (existing)

## Implementation Plan

1. ~~**Create assets module**~~ - Add `assets.py` with `get_static_dir()` function that
   raises on missing assets.

2. ~~**Update server.py**~~ - Remove `static_dir` from `ServerConfig`, always use
   bundled assets.

3. ~~**Update CLI**~~ - Remove `--static-dir` option from `serve` command.

4. ~~**Configure hatchling**~~ - Add `force-include` to bundle frontend assets.

5. ~~**Add build script**~~ - Add `npm run build:bundle` to frontend package.json.

6. ~~**Update .gitignore**~~ - Exclude bundled assets from version control.

7. ~~**Update tests**~~ - Remove tests for `--static-dir`, add tests for bundled assets.

8. ~~**Update documentation**~~ - Update CLAUDE.md with new workflow.

## References

- [Hatchling Build Configuration](https://hatch.pypa.io/latest/config/build/)
- [importlib.resources](https://docs.python.org/3/library/importlib.resources.html)
- [RD-002: Docstage Frontend](RD-002-docstage-frontend.md)
