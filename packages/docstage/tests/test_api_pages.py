"""Tests for pages API endpoint."""

from pathlib import Path

import pytest
from aiohttp.test_utils import TestClient

from docstage.server import ServerConfig, create_app


@pytest.fixture
def docs_dir(tmp_path: Path) -> Path:
    """Create docs directory with sample files."""
    docs = tmp_path / "docs"
    docs.mkdir()
    return docs


@pytest.fixture
def client(tmp_path: Path, docs_dir: Path, aiohttp_client) -> TestClient:
    """Create test client with configured app."""
    config: ServerConfig = {
        "host": "127.0.0.1",
        "port": 8080,
        "source_dir": docs_dir,
        "cache_dir": tmp_path / ".cache",
        "static_dir": None,
    }
    app = create_app(config)
    return aiohttp_client(app)


class TestGetPage:
    """Tests for GET /api/pages/{path}."""

    @pytest.mark.asyncio
    async def test__existing_page__returns_rendered_content(
        self, docs_dir: Path, client
    ) -> None:
        """Return rendered page for existing markdown file."""
        (docs_dir / "guide.md").write_text("# Guide\n\nThis is a guide.")

        test_client = await client
        response = await test_client.get("/api/pages/guide")

        assert response.status == 200
        data = await response.json()
        assert data["meta"]["title"] == "Guide"
        assert data["meta"]["path"] == "/guide"
        assert "This is a guide" in data["content"]

    @pytest.mark.asyncio
    async def test__missing_page__returns_404(self, client) -> None:
        """Return 404 for non-existent page."""
        test_client = await client
        response = await test_client.get("/api/pages/nonexistent")

        assert response.status == 404
        data = await response.json()
        assert data["error"] == "Page not found"
        assert data["path"] == "nonexistent"

    @pytest.mark.asyncio
    async def test__nested_path__returns_page(self, docs_dir: Path, client) -> None:
        """Return page from nested directory."""
        nested = docs_dir / "domain" / "subdomain"
        nested.mkdir(parents=True)
        (nested / "guide.md").write_text("# Nested Guide\n\nDeep content.")

        test_client = await client
        response = await test_client.get("/api/pages/domain/subdomain/guide")

        assert response.status == 200
        data = await response.json()
        assert data["meta"]["title"] == "Nested Guide"
        assert data["meta"]["path"] == "/domain/subdomain/guide"

    @pytest.mark.asyncio
    async def test__index_md__resolves_for_directory_path(
        self, docs_dir: Path, client
    ) -> None:
        """Resolve directory path to index.md."""
        domain = docs_dir / "domain"
        domain.mkdir()
        (domain / "index.md").write_text("# Domain Index\n\nIndex content.")

        test_client = await client
        response = await test_client.get("/api/pages/domain")

        assert response.status == 200
        data = await response.json()
        assert data["meta"]["title"] == "Domain Index"

    @pytest.mark.asyncio
    async def test__response__includes_toc(self, docs_dir: Path, client) -> None:
        """Include table of contents in response."""
        (docs_dir / "guide.md").write_text(
            "# Guide\n\n## Section One\n\nContent.\n\n## Section Two\n\nMore."
        )

        test_client = await client
        response = await test_client.get("/api/pages/guide")

        assert response.status == 200
        data = await response.json()
        assert len(data["toc"]) == 2
        assert data["toc"][0]["title"] == "Section One"
        assert data["toc"][1]["title"] == "Section Two"

    @pytest.mark.asyncio
    async def test__response__includes_breadcrumbs(
        self, docs_dir: Path, client
    ) -> None:
        """Include breadcrumbs in response."""
        nested = docs_dir / "domain" / "subdomain"
        nested.mkdir(parents=True)
        (docs_dir / "domain" / "index.md").write_text("# Domain\n\nContent.")
        (nested / "guide.md").write_text("# Nested Guide\n\nContent.")

        test_client = await client
        response = await test_client.get("/api/pages/domain/subdomain/guide")

        assert response.status == 200
        data = await response.json()
        assert len(data["breadcrumbs"]) == 3
        assert data["breadcrumbs"][0]["title"] == "Domain"
        assert data["breadcrumbs"][0]["path"] == "/domain"
        assert data["breadcrumbs"][2]["path"] == "/domain/subdomain/guide"

    @pytest.mark.asyncio
    async def test__response__includes_cache_headers(
        self, docs_dir: Path, client
    ) -> None:
        """Include cache headers in response."""
        (docs_dir / "guide.md").write_text("# Guide\n\nContent.")

        test_client = await client
        response = await test_client.get("/api/pages/guide")

        assert response.status == 200
        assert "ETag" in response.headers
        assert "Last-Modified" in response.headers
        assert response.headers["Cache-Control"] == "private, max-age=60"

    @pytest.mark.asyncio
    async def test__matching_etag__returns_304(self, docs_dir: Path, client) -> None:
        """Return 304 when ETag matches."""
        (docs_dir / "guide.md").write_text("# Guide\n\nContent.")

        test_client = await client
        response1 = await test_client.get("/api/pages/guide")
        etag = response1.headers["ETag"]

        response2 = await test_client.get(
            "/api/pages/guide", headers={"If-None-Match": etag}
        )

        assert response2.status == 304

    @pytest.mark.asyncio
    async def test__response__meta_includes_last_modified(
        self, docs_dir: Path, client
    ) -> None:
        """Include last_modified in meta."""
        (docs_dir / "guide.md").write_text("# Guide\n\nContent.")

        test_client = await client
        response = await test_client.get("/api/pages/guide")

        assert response.status == 200
        data = await response.json()
        assert "last_modified" in data["meta"]
        assert data["meta"]["last_modified"].endswith("+00:00")
