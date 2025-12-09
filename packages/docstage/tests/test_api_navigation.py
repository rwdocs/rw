"""Tests for navigation API endpoints."""

from pathlib import Path

import pytest
from aiohttp.test_utils import TestClient
from docstage.config import (
    Config,
    DiagramsConfig,
    DocsConfig,
    LiveReloadConfig,
    ServerConfig,
)
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


def _make_config(source_dir: Path, cache_dir: Path) -> Config:
    """Create a Config for testing."""
    return Config(
        server=ServerConfig(),
        docs=DocsConfig(source_dir=source_dir, cache_dir=cache_dir),
        diagrams=DiagramsConfig(),
        live_reload=LiveReloadConfig(enabled=False),
        confluence=None,
        confluence_test=None,
    )


@pytest.fixture
def client(tmp_path: Path, docs_dir: Path, aiohttp_client) -> TestClient:
    """Create test client with configured app."""
    config = _make_config(docs_dir, tmp_path / ".cache")
    app = create_app(config)
    return aiohttp_client(app)


class TestGetNavigation:
    """Tests for GET /api/navigation."""

    @pytest.mark.asyncio
    async def test__populated_docs__returns_full_tree(
        self,
        docs_dir: Path,
        client,
    ) -> None:
        """Return complete navigation tree."""
        test_client = await client
        response = await test_client.get("/api/navigation")

        assert response.status == 200
        data = await response.json()
        assert "items" in data
        assert len(data["items"]) == 2

    @pytest.mark.asyncio
    async def test__tree_structure__includes_nested_items(
        self,
        docs_dir: Path,
        client,
    ) -> None:
        """Include nested navigation items."""
        test_client = await client
        response = await test_client.get("/api/navigation")

        data = await response.json()
        domain_a = next(i for i in data["items"] if i["path"] == "/domain-a")
        assert "children" in domain_a
        assert len(domain_a["children"]) >= 2

    @pytest.mark.asyncio
    async def test__items__include_title_and_path(self, docs_dir: Path, client) -> None:
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
        config = _make_config(docs, tmp_path / ".cache")
        app = create_app(config)
        test_client = await aiohttp_client(app)

        response = await test_client.get("/api/navigation")

        assert response.status == 200
        data = await response.json()
        assert data["items"] == []


class TestGetNavigationSubtree:
    """Tests for GET /api/navigation/{path}."""

    @pytest.mark.asyncio
    async def test__existing_section__returns_subtree(
        self,
        docs_dir: Path,
        client,
    ) -> None:
        """Return subtree for existing section."""
        test_client = await client
        response = await test_client.get("/api/navigation/domain-a")

        assert response.status == 200
        data = await response.json()
        assert "items" in data

    @pytest.mark.asyncio
    async def test__nested_section__returns_children(
        self,
        docs_dir: Path,
        client,
    ) -> None:
        """Return children of nested section."""
        test_client = await client
        response = await test_client.get("/api/navigation/domain-a/subdomain")

        assert response.status == 200
        data = await response.json()
        assert "items" in data
        titles = [item["title"] for item in data["items"]]
        assert "Details" in titles

    @pytest.mark.asyncio
    async def test__nonexistent_section__returns_404(
        self,
        docs_dir: Path,
        client,
    ) -> None:
        """Return 404 for non-existent section."""
        test_client = await client
        response = await test_client.get("/api/navigation/nonexistent")

        assert response.status == 404
        data = await response.json()
        assert data["error"] == "Section not found"
        assert data["path"] == "nonexistent"
