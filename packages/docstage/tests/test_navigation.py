"""Tests for navigation tree builder."""

from pathlib import Path

from docstage.core.cache import FileCache
from docstage.core.navigation import NavigationBuilder, NavItem


class TestNavigationBuilderBuild:
    """Tests for NavigationBuilder.build()."""

    def test__missing_dir__returns_empty_list(self, tmp_path: Path) -> None:
        """Return empty list when source directory doesn't exist."""
        builder = NavigationBuilder(tmp_path / "nonexistent")

        nav = builder.build()

        assert nav == []

    def test__empty_dir__returns_empty_list(self, tmp_path: Path) -> None:
        """Return empty list when source directory is empty."""
        source_dir = tmp_path / "docs"
        source_dir.mkdir()

        builder = NavigationBuilder(source_dir)

        nav = builder.build()

        assert nav == []

    def test__flat_structure__builds_navigation(self, tmp_path: Path) -> None:
        """Build navigation from flat directory with markdown files."""
        source_dir = tmp_path / "docs"
        source_dir.mkdir()
        (source_dir / "guide.md").write_text("# User Guide\n\nContent.")
        (source_dir / "api.md").write_text("# API Reference\n\nDocs.")

        builder = NavigationBuilder(source_dir)

        nav = builder.build()

        assert len(nav) == 2
        titles = [item.title for item in nav]
        assert "API Reference" in titles
        assert "User Guide" in titles

    def test__nested_structure__builds_navigation(self, tmp_path: Path) -> None:
        """Build navigation from nested directory structure."""
        source_dir = tmp_path / "docs"
        domain_dir = source_dir / "domain-a"
        domain_dir.mkdir(parents=True)
        (domain_dir / "index.md").write_text("# Domain A\n\nOverview.")
        (domain_dir / "guide.md").write_text("# Setup Guide\n\nSteps.")

        builder = NavigationBuilder(source_dir)

        nav = builder.build()

        assert len(nav) == 1
        domain = nav[0]
        assert domain.title == "Domain A"
        assert domain.path == "/domain-a"
        assert len(domain.children) == 1
        assert domain.children[0].title == "Setup Guide"
        assert domain.children[0].path == "/domain-a/guide"

    def test__file_with_h1__extracts_title(self, tmp_path: Path) -> None:
        """Extract title from first H1 heading."""
        source_dir = tmp_path / "docs"
        source_dir.mkdir()
        (source_dir / "guide.md").write_text("# My Custom Title\n\nSome content here.")

        builder = NavigationBuilder(source_dir)

        nav = builder.build()

        assert nav[0].title == "My Custom Title"

    def test__file_without_h1__falls_back_to_filename(self, tmp_path: Path) -> None:
        """Fall back to filename when no H1 heading."""
        source_dir = tmp_path / "docs"
        source_dir.mkdir()
        (source_dir / "setup-guide.md").write_text("Content without heading.")

        builder = NavigationBuilder(source_dir)

        nav = builder.build()

        assert nav[0].title == "Setup Guide"

    def test__hidden_files__skips_them(self, tmp_path: Path) -> None:
        """Skip files starting with dot."""
        source_dir = tmp_path / "docs"
        source_dir.mkdir()
        (source_dir / ".hidden.md").write_text("# Hidden\n\nContent.")
        (source_dir / "visible.md").write_text("# Visible\n\nContent.")

        builder = NavigationBuilder(source_dir)

        nav = builder.build()

        assert len(nav) == 1
        assert nav[0].title == "Visible"

    def test__underscore_files__skips_them(self, tmp_path: Path) -> None:
        """Skip files starting with underscore."""
        source_dir = tmp_path / "docs"
        source_dir.mkdir()
        (source_dir / "_partial.md").write_text("# Partial\n\nContent.")
        (source_dir / "main.md").write_text("# Main\n\nContent.")

        builder = NavigationBuilder(source_dir)

        nav = builder.build()

        assert len(nav) == 1
        assert nav[0].title == "Main"

    def test__index_md__skips_as_item(self, tmp_path: Path) -> None:
        """Don't include index.md as separate navigation item."""
        source_dir = tmp_path / "docs"
        domain_dir = source_dir / "domain"
        domain_dir.mkdir(parents=True)
        (domain_dir / "index.md").write_text("# Domain\n\nOverview.")
        (domain_dir / "guide.md").write_text("# Guide\n\nContent.")

        builder = NavigationBuilder(source_dir)

        nav = builder.build()

        assert len(nav) == 1
        assert len(nav[0].children) == 1
        assert nav[0].children[0].title == "Guide"

    def test__empty_directories__skips_them(self, tmp_path: Path) -> None:
        """Skip directories with no markdown files and no index.md."""
        source_dir = tmp_path / "docs"
        empty_dir = source_dir / "empty"
        empty_dir.mkdir(parents=True)
        (source_dir / "guide.md").write_text("# Guide\n\nContent.")

        builder = NavigationBuilder(source_dir)

        nav = builder.build()

        assert len(nav) == 1
        assert nav[0].title == "Guide"

    def test__directory_without_index__promotes_children(
        self,
        tmp_path: Path,
    ) -> None:
        """Promote children to parent level when directory has no index.md."""
        source_dir = tmp_path / "docs"
        # Create directory without index.md
        no_index_dir = source_dir / "no-index"
        no_index_dir.mkdir(parents=True)
        (no_index_dir / "child.md").write_text("# Child Page\n\nContent.")

        builder = NavigationBuilder(source_dir)

        nav = builder.build()

        # Child should be promoted to root level
        assert len(nav) == 1
        assert nav[0].title == "Child Page"
        assert nav[0].path == "/no-index/child"

    def test__nested_dir_without_index__promotes_nested_children(
        self,
        tmp_path: Path,
    ) -> None:
        """Promote nested navigable items when intermediate directory has no index.md."""
        source_dir = tmp_path / "docs"
        # Create: docs/wrapper/domain-a/index.md where wrapper has no index.md
        wrapper_dir = source_dir / "wrapper"
        domain_dir = wrapper_dir / "domain-a"
        domain_dir.mkdir(parents=True)
        (domain_dir / "index.md").write_text("# Domain A\n\nOverview.")

        builder = NavigationBuilder(source_dir)

        nav = builder.build()

        # domain-a should be promoted to root level
        assert len(nav) == 1
        assert nav[0].title == "Domain A"
        assert nav[0].path == "/wrapper/domain-a"

    def test__with_cache__uses_cached_result(self, tmp_path: Path) -> None:
        """Use cached navigation on subsequent builds."""
        source_dir = tmp_path / "docs"
        source_dir.mkdir()
        (source_dir / "guide.md").write_text("# Guide\n\nContent.")

        cache = FileCache(tmp_path / ".cache")
        builder = NavigationBuilder(source_dir, cache)

        builder.build()
        # Modify file after first build
        (source_dir / "new.md").write_text("# New\n\nContent.")
        nav = builder.build()

        # Should return cached version
        assert len(nav) == 1

    def test__use_cache_false__bypasses_cache(self, tmp_path: Path) -> None:
        """Bypass cache when use_cache=False."""
        source_dir = tmp_path / "docs"
        source_dir.mkdir()
        (source_dir / "guide.md").write_text("# Guide\n\nContent.")

        cache = FileCache(tmp_path / ".cache")
        builder = NavigationBuilder(source_dir, cache)

        builder.build()
        (source_dir / "new.md").write_text("# New\n\nContent.")
        nav = builder.build(use_cache=False)

        assert len(nav) == 2


class TestNavigationBuilderGetSubtree:
    """Tests for NavigationBuilder.get_subtree()."""

    def test__valid_path__returns_subtree(self, tmp_path: Path) -> None:
        """Return subtree for specific section path."""
        source_dir = tmp_path / "docs"
        domain_dir = source_dir / "domain-a" / "sub"
        domain_dir.mkdir(parents=True)
        (source_dir / "domain-a" / "index.md").write_text("# Domain A")
        (domain_dir / "index.md").write_text("# Sub")
        (domain_dir / "guide.md").write_text("# Guide")

        builder = NavigationBuilder(source_dir)

        subtree = builder.get_subtree("domain-a")

        assert subtree is not None
        assert len(subtree) == 1
        assert subtree[0].title == "Sub"

    def test__invalid_path__returns_none(self, tmp_path: Path) -> None:
        """Return None when path doesn't exist in tree."""
        source_dir = tmp_path / "docs"
        source_dir.mkdir()
        (source_dir / "guide.md").write_text("# Guide")

        builder = NavigationBuilder(source_dir)

        subtree = builder.get_subtree("nonexistent")

        assert subtree is None


class TestNavigationBuilderInvalidate:
    """Tests for NavigationBuilder.invalidate()."""

    def test__with_cache__invalidates_cached_navigation(self, tmp_path: Path) -> None:
        """Invalidate cached navigation tree."""
        source_dir = tmp_path / "docs"
        source_dir.mkdir()
        (source_dir / "guide.md").write_text("# Guide")

        cache = FileCache(tmp_path / ".cache")
        builder = NavigationBuilder(source_dir, cache)

        builder.build()
        (source_dir / "new.md").write_text("# New")
        builder.invalidate()
        nav = builder.build()

        assert len(nav) == 2


class TestNavItem:
    """Tests for NavItem dataclass."""

    def test__creation__stores_title_and_path(self) -> None:
        """NavItem stores title and path."""
        item = NavItem(title="Guide", path="/guide")

        assert item.title == "Guide"
        assert item.path == "/guide"
        assert item.children == []

    def test__with_children__stores_children(self) -> None:
        """NavItem stores children."""
        child = NavItem(title="Child", path="/parent/child")
        item = NavItem(title="Parent", path="/parent", children=[child])

        assert len(item.children) == 1
        assert item.children[0].title == "Child"

    def test__to_dict__without_children(self) -> None:
        """Convert item without children to dict."""
        item = NavItem(title="Guide", path="/guide")

        result = item.to_dict()

        assert result == {"title": "Guide", "path": "/guide"}

    def test__to_dict__with_children(self) -> None:
        """Convert item with children to dict."""
        child = NavItem(title="Child", path="/parent/child")
        item = NavItem(title="Parent", path="/parent", children=[child])

        result = item.to_dict()

        assert result == {
            "title": "Parent",
            "path": "/parent",
            "children": [{"title": "Child", "path": "/parent/child"}],
        }


class TestNavigationBuilderProperties:
    """Tests for NavigationBuilder properties."""

    def test__source_dir__returns_path(self, tmp_path: Path) -> None:
        """Return source directory from property."""
        source_dir = tmp_path / "docs"
        builder = NavigationBuilder(source_dir)

        assert builder.source_dir == source_dir


class TestNavigationBuilderExtractTitle:
    """Tests for title extraction edge cases."""

    def test__unreadable_file__falls_back_to_filename(self, tmp_path: Path) -> None:
        """Fall back to filename when file cannot be read."""
        source_dir = tmp_path / "docs"
        source_dir.mkdir()
        file_path = source_dir / "guide.md"
        file_path.write_text("# Guide Title\n\nContent.")
        # Make file unreadable
        file_path.chmod(0o000)

        builder = NavigationBuilder(source_dir)

        try:
            nav = builder.build()
            # Should fall back to filename-based title
            assert nav[0].title == "Guide"
        finally:
            # Restore permissions for cleanup
            file_path.chmod(0o644)


class TestNavigationBuilderInvalidateNoCache:
    """Tests for invalidate without cache."""

    def test__no_cache__invalidate_is_noop(self, tmp_path: Path) -> None:
        """Invalidate is a no-op when no cache is configured."""
        source_dir = tmp_path / "docs"
        source_dir.mkdir()
        builder = NavigationBuilder(source_dir)

        # Should not raise
        builder.invalidate()


class TestNavigationBuilderGetSubtreeEdgeCases:
    """Tests for get_subtree edge cases."""

    def test__empty_path__returns_full_navigation(self, tmp_path: Path) -> None:
        """Return full navigation for empty path."""
        source_dir = tmp_path / "docs"
        source_dir.mkdir()
        (source_dir / "guide.md").write_text("# Guide\n\nContent.")

        builder = NavigationBuilder(source_dir)

        subtree = builder.get_subtree("")

        assert subtree is not None
        assert len(subtree) == 1
        assert subtree[0].title == "Guide"

    def test__leading_slash__returns_subtree(self, tmp_path: Path) -> None:
        """Handle path with leading slash."""
        source_dir = tmp_path / "docs"
        domain_dir = source_dir / "domain"
        domain_dir.mkdir(parents=True)
        (domain_dir / "index.md").write_text("# Domain")
        (domain_dir / "guide.md").write_text("# Guide")

        builder = NavigationBuilder(source_dir)

        subtree = builder.get_subtree("/domain")

        assert subtree is not None
        assert len(subtree) == 1
        assert subtree[0].title == "Guide"

    def test__deeply_nested_path__returns_subtree(self, tmp_path: Path) -> None:
        """Navigate through deeply nested structure."""
        source_dir = tmp_path / "docs"
        deep_dir = source_dir / "a" / "b" / "c"
        deep_dir.mkdir(parents=True)
        (source_dir / "a" / "index.md").write_text("# A")
        (source_dir / "a" / "b" / "index.md").write_text("# B")
        (deep_dir / "index.md").write_text("# C")
        (deep_dir / "file.md").write_text("# File")

        builder = NavigationBuilder(source_dir)

        subtree = builder.get_subtree("a/b/c")

        assert subtree is not None
        assert len(subtree) == 1
        assert subtree[0].title == "File"


class TestNavigationBuilderTitleFromName:
    """Tests for title generation from filename."""

    def test__snake_case__converts_to_title_case(self, tmp_path: Path) -> None:
        """Convert snake_case to Title Case."""
        source_dir = tmp_path / "docs"
        source_dir.mkdir()
        (source_dir / "my_great_guide.md").write_text("Content without heading.")

        builder = NavigationBuilder(source_dir)

        nav = builder.build()

        assert nav[0].title == "My Great Guide"

    def test__mixed_separators__converts_to_title_case(self, tmp_path: Path) -> None:
        """Handle both hyphens and underscores."""
        source_dir = tmp_path / "docs"
        source_dir.mkdir()
        (source_dir / "api-user_guide.md").write_text("No heading here.")

        builder = NavigationBuilder(source_dir)

        nav = builder.build()

        assert nav[0].title == "Api User Guide"


class TestNavigationBuilderSorting:
    """Tests for sorting behavior."""

    def test__mixed_dirs_and_files__directories_first(self, tmp_path: Path) -> None:
        """Directories appear before files in navigation."""
        source_dir = tmp_path / "docs"
        source_dir.mkdir()
        # Create file first, then directory
        (source_dir / "zebra.md").write_text("# Zebra")
        subdir = source_dir / "aardvark"
        subdir.mkdir()
        (subdir / "index.md").write_text("# Aardvark")

        builder = NavigationBuilder(source_dir)

        nav = builder.build()

        # Directory should come first despite alphabetical order
        assert nav[0].title == "Aardvark"
        assert nav[1].title == "Zebra"

    def test__mixed_case_names__sorted_case_insensitive(self, tmp_path: Path) -> None:
        """Items sorted case-insensitively."""
        source_dir = tmp_path / "docs"
        source_dir.mkdir()
        (source_dir / "Zebra.md").write_text("# Zebra")
        (source_dir / "apple.md").write_text("# Apple")
        (source_dir / "Banana.md").write_text("# Banana")

        builder = NavigationBuilder(source_dir)

        nav = builder.build()

        titles = [item.title for item in nav]
        assert titles == ["Apple", "Banana", "Zebra"]
