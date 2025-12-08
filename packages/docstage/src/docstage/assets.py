"""Asset discovery for bundled frontend assets.

Locates static assets bundled into the docstage package at build time.
"""

from importlib.resources import files
from pathlib import Path


def get_static_dir() -> Path:
    """Return path to bundled static assets.

    Returns:
        Path to the static directory containing frontend assets.

    Raises:
        FileNotFoundError: If static assets are not bundled.
    """
    static = files("docstage").joinpath("static")
    if not static.is_dir():
        msg = (
            "Bundled static assets not found. "
            "Run 'cd frontend && npm run build:bundle' then 'uv sync --reinstall'."
        )
        raise FileNotFoundError(msg)
    return Path(str(static))
