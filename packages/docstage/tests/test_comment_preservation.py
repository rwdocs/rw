"""Tests for Confluence comment preservation."""

from docstage.confluence.comment_preservation import (
    CommentMarkerTransfer,
    CommentPreserver,
    ConfluenceTreeParser,
    ConfluenceTreeSerializer,
    TreeMatcher,
    TreeNode,
)


class TestConfluenceTreeParser:
    """Tests for ConfluenceTreeParser."""

    def test__parse__simple_element(self) -> None:
        """Parser should parse simple HTML element."""
        parser = ConfluenceTreeParser()
        tree = parser.parse("<p>Hello</p>")

        assert len(tree.children) == 1
        p_node = tree.children[0]
        assert p_node.tag == "p"
        assert p_node.text == "Hello"

    def test__parse__nested_elements(self) -> None:
        """Parser should parse nested elements."""
        parser = ConfluenceTreeParser()
        tree = parser.parse("<p><strong>Bold</strong> text</p>")

        p_node = tree.children[0]
        assert p_node.tag == "p"
        assert p_node.text == ""
        assert len(p_node.children) == 1

        strong_node = p_node.children[0]
        assert strong_node.tag == "strong"
        assert strong_node.text == "Bold"
        assert strong_node.tail == " text"

    def test__parse__comment_marker(self) -> None:
        """Parser should identify inline comment markers."""
        parser = ConfluenceTreeParser()
        html = '<p><ac:inline-comment-marker ac:ref="abc">marked</ac:inline-comment-marker> text</p>'
        tree = parser.parse(html)

        p_node = tree.children[0]
        marker = p_node.children[0]
        assert marker.is_comment_marker()
        assert marker.text == "marked"
        assert marker.tail == " text"

    def test__parse__html_entities(self) -> None:
        """Parser should convert HTML entities to Unicode."""
        parser = ConfluenceTreeParser()
        tree = parser.parse("<p>Hello&nbsp;World&mdash;Test</p>")

        p_node = tree.children[0]
        assert "\u00a0" in p_node.text  # nbsp
        assert "\u2014" in p_node.text  # mdash


class TestTreeNode:
    """Tests for TreeNode."""

    def test__get_text_signature__direct_text(self) -> None:
        """Should return direct text content."""
        node = TreeNode(tag="p", text="Hello World")
        assert node.get_text_signature() == "Hello World"

    def test__get_text_signature__with_children(self) -> None:
        """Should include text from children."""
        child = TreeNode(tag="strong", text="Bold", tail=" text")
        node = TreeNode(tag="p", text="", children=[child])
        assert node.get_text_signature() == "Bold text"

    def test__get_text_signature__with_tail(self) -> None:
        """Should include tail text."""
        node = TreeNode(tag="span", text="Hello", tail=" World")
        assert node.get_text_signature() == "Hello World"

    def test__is_comment_marker__true(self) -> None:
        """Should identify comment markers by tag."""
        node = TreeNode(
            tag="{http://www.atlassian.com/schema/confluence/4/ac/}inline-comment-marker"
        )
        assert node.is_comment_marker()

    def test__is_comment_marker__prefixed(self) -> None:
        """Should identify prefixed comment markers."""
        node = TreeNode(tag="ac:inline-comment-marker")
        assert node.is_comment_marker()

    def test__is_comment_marker__false(self) -> None:
        """Should return False for non-marker tags."""
        node = TreeNode(tag="p")
        assert not node.is_comment_marker()


class TestTreeMatcher:
    """Tests for TreeMatcher."""

    def test__match__identical_trees(self) -> None:
        """Should match all nodes in identical trees."""
        parser = ConfluenceTreeParser()
        old_tree = parser.parse("<p>Hello</p>")
        new_tree = parser.parse("<p>Hello</p>")

        matcher = TreeMatcher(old_tree, new_tree)
        matches = matcher.match()

        assert len(matches) == 1

    def test__match__different_text(self) -> None:
        """Should not match nodes with significantly different text."""
        parser = ConfluenceTreeParser()
        old_tree = parser.parse("<p>Hello World</p>")
        new_tree = parser.parse("<p>Completely different</p>")

        matcher = TreeMatcher(old_tree, new_tree)
        matches = matcher.match()

        assert len(matches) == 0

    def test__match__ignores_comment_markers_in_old(self) -> None:
        """Should match content ignoring comment markers."""
        parser = ConfluenceTreeParser()
        old_html = '<p><ac:inline-comment-marker ac:ref="x">marked</ac:inline-comment-marker> text</p>'
        new_html = "<p>marked text</p>"

        old_tree = parser.parse(old_html)
        new_tree = parser.parse(new_html)

        matcher = TreeMatcher(old_tree, new_tree)
        matches = matcher.match()

        # p should match
        assert len(matches) == 1


class TestCommentMarkerTransfer:
    """Tests for CommentMarkerTransfer."""

    def test__transfer__marker_in_direct_text(self) -> None:
        """Should transfer marker when text is in node's direct text."""
        parser = ConfluenceTreeParser()
        old_html = '<p><ac:inline-comment-marker ac:ref="abc">marked</ac:inline-comment-marker> text</p>'
        new_html = "<p>marked text</p>"

        old_tree = parser.parse(old_html)
        new_tree = parser.parse(new_html)

        # Manually match p nodes
        old_p = old_tree.children[0]
        new_p = new_tree.children[0]
        matches = {id(old_p): new_p}

        transfer = CommentMarkerTransfer()
        transfer.transfer(matches, new_tree, old_tree)

        assert len(transfer.unmatched_comments) == 0
        assert len(new_p.children) == 1
        assert new_p.children[0].is_comment_marker()

    def test__transfer__marker_in_child_tail(self) -> None:
        """Should transfer marker when text is in child's tail."""
        parser = ConfluenceTreeParser()
        # Old: <li><code>x</code> <marker>marked</marker>, rest</li>
        old_html = '<li><code>x</code> <ac:inline-comment-marker ac:ref="abc">marked</ac:inline-comment-marker>, rest</li>'
        # New: <li><code>x</code> marked, rest</li>
        new_html = "<li><code>x</code> marked, rest</li>"

        old_tree = parser.parse(old_html)
        new_tree = parser.parse(new_html)

        # Match li nodes
        old_li = old_tree.children[0]
        new_li = new_tree.children[0]
        matches = {id(old_li): new_li}

        transfer = CommentMarkerTransfer()
        transfer.transfer(matches, new_tree, old_tree)

        assert len(transfer.unmatched_comments) == 0
        # Should have code and marker as children
        assert len(new_li.children) == 2
        assert new_li.children[0].tag == "code"
        assert new_li.children[1].is_comment_marker()
        assert new_li.children[1].text == "marked"

    def test__transfer__marker_not_found(self) -> None:
        """Should track unmatched comments when text not found."""
        parser = ConfluenceTreeParser()
        old_html = '<p><ac:inline-comment-marker ac:ref="abc">original</ac:inline-comment-marker></p>'
        new_html = "<p>completely different text</p>"

        old_tree = parser.parse(old_html)
        new_tree = parser.parse(new_html)

        old_p = old_tree.children[0]
        new_p = new_tree.children[0]
        matches = {id(old_p): new_p}

        transfer = CommentMarkerTransfer()
        transfer.transfer(matches, new_tree, old_tree)

        assert len(transfer.unmatched_comments) == 1
        assert transfer.unmatched_comments[0].text == "original"


class TestConfluenceTreeSerializer:
    """Tests for ConfluenceTreeSerializer."""

    def test__serialize__simple_element(self) -> None:
        """Should serialize simple element back to HTML."""
        node = TreeNode(tag="root", children=[TreeNode(tag="p", text="Hello")])
        serializer = ConfluenceTreeSerializer()

        html = serializer.serialize(node)
        assert html == "<p>Hello</p>"

    def test__serialize__with_children(self) -> None:
        """Should serialize nested elements."""
        strong = TreeNode(tag="strong", text="Bold", tail=" text")
        p = TreeNode(tag="p", children=[strong])
        root = TreeNode(tag="root", children=[p])

        serializer = ConfluenceTreeSerializer()
        html = serializer.serialize(root)

        assert html == "<p><strong>Bold</strong> text</p>"


class TestCommentPreserver:
    """Tests for CommentPreserver end-to-end."""

    def test__preserve_comments__simple_case(self) -> None:
        """Should preserve comment markers in simple case."""
        old_html = '<p><ac:inline-comment-marker ac:ref="abc">marked</ac:inline-comment-marker> text</p>'
        new_html = "<p>marked text</p>"

        preserver = CommentPreserver()
        result = preserver.preserve_comments(old_html, new_html)

        assert len(result.unmatched_comments) == 0
        assert "ac:inline-comment-marker" in result.html
        assert 'ac:ref="abc"' in result.html

    def test__preserve_comments__marker_in_tail(self) -> None:
        """Should preserve marker when text is in element tail."""
        old_html = '<li><code>x</code> <ac:inline-comment-marker ac:ref="id">marked</ac:inline-comment-marker>, rest</li>'
        new_html = "<li><code>x</code> marked, rest</li>"

        preserver = CommentPreserver()
        result = preserver.preserve_comments(old_html, new_html)

        assert len(result.unmatched_comments) == 0
        assert "ac:inline-comment-marker" in result.html

    def test__preserve_comments__cyrillic_text(self) -> None:
        """Should handle Cyrillic text in markers."""
        old_html = '<li><code>gateway</code> <ac:inline-comment-marker ac:ref="xyz">проверяет тип</ac:inline-comment-marker>, активность</li>'
        new_html = "<li><code>gateway</code> проверяет тип, активность</li>"

        preserver = CommentPreserver()
        result = preserver.preserve_comments(old_html, new_html)

        assert len(result.unmatched_comments) == 0
        assert "проверяет тип" in result.html
        assert "ac:inline-comment-marker" in result.html

    def test__preserve_comments__multiple_markers_in_different_elements(self) -> None:
        """Should preserve multiple markers in different elements."""
        old_html = '<p><ac:inline-comment-marker ac:ref="a">first paragraph text</ac:inline-comment-marker></p><p><ac:inline-comment-marker ac:ref="b">second paragraph text</ac:inline-comment-marker></p>'
        new_html = "<p>first paragraph text</p><p>second paragraph text</p>"

        preserver = CommentPreserver()
        result = preserver.preserve_comments(old_html, new_html)

        assert len(result.unmatched_comments) == 0
        # Count opening tags only (closing tags also contain the string)
        assert result.html.count("<ac:inline-comment-marker") == 2

    def test__preserve_comments__unmatched_when_text_removed(self) -> None:
        """Should return unmatched when marker text is removed from document."""
        # The marked text "original" exists in old but not in new
        # Paragraphs still match (>80% similar) but the specific text is gone
        old_html = '<p>Some text with <ac:inline-comment-marker ac:ref="abc">original word</ac:inline-comment-marker> in it</p>'
        new_html = "<p>Some text with different word in it</p>"

        preserver = CommentPreserver()
        result = preserver.preserve_comments(old_html, new_html)

        assert len(result.unmatched_comments) == 1
        assert result.unmatched_comments[0].ref == "abc"
        assert result.unmatched_comments[0].text == "original word"

    def test__preserve_comments__unmatched_when_parent_not_matched(self) -> None:
        """Should report unmatched when parent node doesn't match."""
        # The paragraph text is completely different, so nodes don't match
        old_html = '<p><ac:inline-comment-marker ac:ref="xyz">Original sentence here</ac:inline-comment-marker></p>'
        new_html = "<p>Completely different content</p>"

        preserver = CommentPreserver()
        result = preserver.preserve_comments(old_html, new_html)

        # Should report as unmatched because parent didn't match
        assert len(result.unmatched_comments) == 1
        assert result.unmatched_comments[0].ref == "xyz"
        assert result.unmatched_comments[0].text == "Original sentence here"
