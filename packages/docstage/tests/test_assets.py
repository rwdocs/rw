"""Tests for assets module."""

from importlib.resources import files

import pytest
from docstage.assets import get_static_dir


def _bundled_assets_exist() -> bool:
    """Check if bundled frontend assets exist."""
    static = files("docstage").joinpath("static")
    return static.is_dir() and (static / "index.html").is_file()


requires_bundled_assets = pytest.mark.skipif(
    not _bundled_assets_exist(),
    reason="Bundled assets not found. Run 'cd frontend && npm run build:bundle'.",
)


class TestGetStaticDir:
    """Tests for get_static_dir()."""

    @requires_bundled_assets
    def test__bundled_assets_exist__returns_path(self) -> None:
        """Bundled assets directory should be accessible."""
        static_dir = get_static_dir()

        assert static_dir.exists()
        assert (static_dir / "index.html").exists()

    def test__static_not_directory__raises_file_not_found_error(
        self,
        monkeypatch: pytest.MonkeyPatch,
    ) -> None:
        """Should raise FileNotFoundError when static directory doesn't exist."""

        class FakeTraversable:
            def is_dir(self) -> bool:
                return False

            def joinpath(self, name: str) -> FakeTraversable:
                return self

        monkeypatch.setattr("docstage.assets.files", lambda _: FakeTraversable())

        with pytest.raises(
            FileNotFoundError,
            match="Bundled static assets not found",
        ):
            get_static_dir()
