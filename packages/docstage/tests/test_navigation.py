"""Tests for navigation tree builder."""

from pathlib import Path

from docstage.core.cache import FileCache
from docstage.core.navigation import NavItem, NavigationBuilder, NavigationTree


class TestNavigationBuilderBuild:
    """Tests for NavigationBuilder.build()."""

    def test_returns_empty_tree_for_missing_dir(self, tmp_path: Path) -> None:
        """Return empty tree when source directory doesn't exist."""
        builder = NavigationBuilder(tmp_path / "nonexistent")

        tree = builder.build()

        assert tree.items == []

    def test_returns_empty_tree_for_empty_dir(self, tmp_path: Path) -> None:
        """Return empty tree when source directory is empty."""
        source_dir = tmp_path / "docs"
        source_dir.mkdir()

        builder = NavigationBuilder(source_dir)

        tree = builder.build()

        assert tree.items == []

    def test_builds_flat_structure(self, tmp_path: Path) -> None:
        """Build tree from flat directory with markdown files."""
        source_dir = tmp_path / "docs"
        source_dir.mkdir()
        (source_dir / "guide.md").write_text("# User Guide\n\nContent.")
        (source_dir / "api.md").write_text("# API Reference\n\nDocs.")

        builder = NavigationBuilder(source_dir)

        tree = builder.build()

        assert len(tree.items) == 2
        titles = [item.title for item in tree.items]
        assert "API Reference" in titles
        assert "User Guide" in titles

    def test_builds_nested_structure(self, tmp_path: Path) -> None:
        """Build tree from nested directory structure."""
        source_dir = tmp_path / "docs"
        domain_dir = source_dir / "domain-a"
        domain_dir.mkdir(parents=True)
        (domain_dir / "index.md").write_text("# Domain A\n\nOverview.")
        (domain_dir / "guide.md").write_text("# Setup Guide\n\nSteps.")

        builder = NavigationBuilder(source_dir)

        tree = builder.build()

        assert len(tree.items) == 1
        domain = tree.items[0]
        assert domain.title == "Domain A"
        assert domain.path == "/domain-a"
        assert len(domain.children) == 1
        assert domain.children[0].title == "Setup Guide"
        assert domain.children[0].path == "/domain-a/guide"

    def test_extracts_title_from_h1(self, tmp_path: Path) -> None:
        """Extract title from first H1 heading."""
        source_dir = tmp_path / "docs"
        source_dir.mkdir()
        (source_dir / "guide.md").write_text("# My Custom Title\n\nSome content here.")

        builder = NavigationBuilder(source_dir)

        tree = builder.build()

        assert tree.items[0].title == "My Custom Title"

    def test_falls_back_to_filename(self, tmp_path: Path) -> None:
        """Fall back to filename when no H1 heading."""
        source_dir = tmp_path / "docs"
        source_dir.mkdir()
        (source_dir / "setup-guide.md").write_text("Content without heading.")

        builder = NavigationBuilder(source_dir)

        tree = builder.build()

        assert tree.items[0].title == "Setup Guide"

    def test_skips_hidden_files(self, tmp_path: Path) -> None:
        """Skip files starting with dot."""
        source_dir = tmp_path / "docs"
        source_dir.mkdir()
        (source_dir / ".hidden.md").write_text("# Hidden\n\nContent.")
        (source_dir / "visible.md").write_text("# Visible\n\nContent.")

        builder = NavigationBuilder(source_dir)

        tree = builder.build()

        assert len(tree.items) == 1
        assert tree.items[0].title == "Visible"

    def test_skips_underscore_files(self, tmp_path: Path) -> None:
        """Skip files starting with underscore."""
        source_dir = tmp_path / "docs"
        source_dir.mkdir()
        (source_dir / "_partial.md").write_text("# Partial\n\nContent.")
        (source_dir / "main.md").write_text("# Main\n\nContent.")

        builder = NavigationBuilder(source_dir)

        tree = builder.build()

        assert len(tree.items) == 1
        assert tree.items[0].title == "Main"

    def test_skips_index_md_as_item(self, tmp_path: Path) -> None:
        """Don't include index.md as separate navigation item."""
        source_dir = tmp_path / "docs"
        domain_dir = source_dir / "domain"
        domain_dir.mkdir(parents=True)
        (domain_dir / "index.md").write_text("# Domain\n\nOverview.")
        (domain_dir / "guide.md").write_text("# Guide\n\nContent.")

        builder = NavigationBuilder(source_dir)

        tree = builder.build()

        assert len(tree.items) == 1
        domain = tree.items[0]
        assert len(domain.children) == 1
        assert domain.children[0].title == "Guide"

    def test_skips_empty_directories(self, tmp_path: Path) -> None:
        """Skip directories with no markdown files and no index.md."""
        source_dir = tmp_path / "docs"
        empty_dir = source_dir / "empty"
        empty_dir.mkdir(parents=True)
        (source_dir / "guide.md").write_text("# Guide\n\nContent.")

        builder = NavigationBuilder(source_dir)

        tree = builder.build()

        assert len(tree.items) == 1
        assert tree.items[0].title == "Guide"

    def test_uses_cache(self, tmp_path: Path) -> None:
        """Use cached navigation on subsequent builds."""
        source_dir = tmp_path / "docs"
        source_dir.mkdir()
        (source_dir / "guide.md").write_text("# Guide\n\nContent.")

        cache = FileCache(tmp_path / ".cache")
        builder = NavigationBuilder(source_dir, cache)

        builder.build()
        # Modify file after first build
        (source_dir / "new.md").write_text("# New\n\nContent.")
        tree = builder.build()

        # Should return cached version
        assert len(tree.items) == 1

    def test_bypasses_cache_when_disabled(self, tmp_path: Path) -> None:
        """Bypass cache when use_cache=False."""
        source_dir = tmp_path / "docs"
        source_dir.mkdir()
        (source_dir / "guide.md").write_text("# Guide\n\nContent.")

        cache = FileCache(tmp_path / ".cache")
        builder = NavigationBuilder(source_dir, cache)

        builder.build()
        (source_dir / "new.md").write_text("# New\n\nContent.")
        tree = builder.build(use_cache=False)

        assert len(tree.items) == 2


class TestNavigationBuilderGetSubtree:
    """Tests for NavigationBuilder.get_subtree()."""

    def test_returns_subtree_for_path(self, tmp_path: Path) -> None:
        """Return subtree for specific section path."""
        source_dir = tmp_path / "docs"
        domain_dir = source_dir / "domain-a" / "sub"
        domain_dir.mkdir(parents=True)
        (source_dir / "domain-a" / "index.md").write_text("# Domain A")
        (domain_dir / "guide.md").write_text("# Guide")

        builder = NavigationBuilder(source_dir)

        subtree = builder.get_subtree("domain-a")

        assert subtree is not None
        assert len(subtree.items) == 1
        assert subtree.items[0].title == "Sub"

    def test_returns_none_for_invalid_path(self, tmp_path: Path) -> None:
        """Return None when path doesn't exist in tree."""
        source_dir = tmp_path / "docs"
        source_dir.mkdir()
        (source_dir / "guide.md").write_text("# Guide")

        builder = NavigationBuilder(source_dir)

        subtree = builder.get_subtree("nonexistent")

        assert subtree is None


class TestNavigationBuilderInvalidate:
    """Tests for NavigationBuilder.invalidate()."""

    def test_invalidates_cached_navigation(self, tmp_path: Path) -> None:
        """Invalidate cached navigation tree."""
        source_dir = tmp_path / "docs"
        source_dir.mkdir()
        (source_dir / "guide.md").write_text("# Guide")

        cache = FileCache(tmp_path / ".cache")
        builder = NavigationBuilder(source_dir, cache)

        builder.build()
        (source_dir / "new.md").write_text("# New")
        builder.invalidate()
        tree = builder.build()

        assert len(tree.items) == 2


class TestNavItem:
    """Tests for NavItem dataclass."""

    def test_to_dict_minimal(self) -> None:
        """Convert item without children to dict."""
        item = NavItem(title="Guide", path="/guide")

        result = item.to_dict()

        assert result == {"title": "Guide", "path": "/guide"}

    def test_to_dict_with_children(self) -> None:
        """Convert item with children to dict."""
        child = NavItem(title="Sub", path="/parent/sub")
        item = NavItem(title="Parent", path="/parent", children=[child])

        result = item.to_dict()

        assert result == {
            "title": "Parent",
            "path": "/parent",
            "children": [
                {"title": "Sub", "path": "/parent/sub"},
            ],
        }


class TestNavigationTree:
    """Tests for NavigationTree dataclass."""

    def test_to_dict(self) -> None:
        """Convert tree to dict."""
        item = NavItem(title="Guide", path="/guide")
        tree = NavigationTree(items=[item])

        result = tree.to_dict()

        assert result == {
            "items": [
                {"title": "Guide", "path": "/guide"},
            ],
        }
