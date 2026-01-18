"""Tests for navigation API endpoints."""

from pathlib import Path

import pytest
from docstage.config import Config
from docstage.server import create_app


@pytest.fixture
def docs_dir(tmp_path: Path) -> Path:
    """Create docs directory with sample structure."""
    docs = tmp_path / "docs"
    docs.mkdir()

    domain_a = docs / "domain-a"
    domain_a.mkdir()
    (domain_a / "index.md").write_text("# Domain A\n\nIndex content.")
    (domain_a / "guide.md").write_text("# Guide\n\nGuide content.")

    subdomain = domain_a / "subdomain"
    subdomain.mkdir()
    (subdomain / "index.md").write_text("# Subdomain\n\nSubdomain index.")
    (subdomain / "details.md").write_text("# Details\n\nDetails content.")

    domain_b = docs / "domain-b"
    domain_b.mkdir()
    (domain_b / "api.md").write_text("# API Docs\n\nAPI content.")

    return docs


@pytest.fixture
def client(
    tmp_path: Path,
    docs_dir: Path,
    aiohttp_client,
):
    """Create test client with configured app."""
    config_file = tmp_path / "docstage.toml"
    config_file.write_text("")
    config = Config.load(
        config_file,
        source_dir=docs_dir,
        cache_dir=tmp_path / ".cache",
        live_reload_enabled=False,
    )
    app = create_app(config)
    return aiohttp_client(app)


class TestGetNavigation:
    """Tests for GET /api/navigation."""

    @pytest.mark.asyncio
    async def test__populated_docs__returns_full_tree(self, client) -> None:
        """Return complete navigation tree."""
        test_client = await client
        response = await test_client.get("/api/navigation")

        assert response.status == 200
        data = await response.json()
        assert "items" in data
        assert len(data["items"]) == 2

    @pytest.mark.asyncio
    async def test__tree_structure__includes_nested_items(self, client) -> None:
        """Include nested navigation items."""
        test_client = await client
        response = await test_client.get("/api/navigation")

        data = await response.json()
        domain_a = next(i for i in data["items"] if i["path"] == "/domain-a")
        assert "children" in domain_a
        assert len(domain_a["children"]) >= 2

    @pytest.mark.asyncio
    async def test__items__include_title_and_path(self, client) -> None:
        """Each item has title and path."""
        test_client = await client
        response = await test_client.get("/api/navigation")

        data = await response.json()
        for item in data["items"]:
            assert "title" in item
            assert "path" in item

    @pytest.mark.asyncio
    async def test__empty_docs__returns_empty_items(
        self,
        tmp_path: Path,
        aiohttp_client,
    ) -> None:
        """Return empty items for empty docs directory."""
        docs = tmp_path / "empty-docs"
        docs.mkdir()
        config_file = tmp_path / "docstage.toml"
        config_file.write_text("")
        config = Config.load(
            config_file,
            source_dir=docs,
            cache_dir=tmp_path / ".cache",
            live_reload_enabled=False,
        )
        app = create_app(config)
        test_client = await aiohttp_client(app)

        response = await test_client.get("/api/navigation")

        assert response.status == 200
        data = await response.json()
        assert data["items"] == []
