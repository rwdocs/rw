"""Comment preservation for Confluence page updates.

This module preserves inline comment markers when updating Confluence pages from markdown.
It uses tree-based comparison to match content between old and new HTML and transfers
comment markers to matching positions.
"""

import logging
from dataclasses import dataclass, field
from difflib import SequenceMatcher
from typing import Optional
from xml.etree import ElementTree as ET

logger = logging.getLogger(__name__)


@dataclass
class TreeNode:
    """Represents a node in the HTML tree."""

    tag: str  # Element tag name
    text: str = ''  # Direct text content
    tail: str = ''  # Text after element (for inline elements)
    attrs: dict[str, str] = field(default_factory=dict)  # Attributes
    children: list['TreeNode'] = field(default_factory=list)  # Child nodes
    element: Optional[ET.Element] = None  # Original XML element reference

    def get_text_signature(self) -> str:
        """Get normalized text content for matching.

        Returns:
            Concatenated text from this node and all descendants
        """
        texts = []
        if self.text:
            texts.append(self.text.strip())
        for child in self.children:
            texts.append(child.get_text_signature())
        if self.tail:
            texts.append(self.tail.strip())
        return ' '.join(t for t in texts if t)

    def is_comment_marker(self) -> bool:
        """Check if this is an inline comment marker.

        Returns:
            True if this node is a Confluence inline comment marker
        """
        # Handle both with and without namespace prefix
        return (
            self.tag == '{http://www.atlassian.com/schema/confluence/4/ac/}inline-comment-marker'
            or self.tag == 'ac:inline-comment-marker'
            or 'inline-comment-marker' in self.tag
        )

    def get_comment_markers(self) -> list['TreeNode']:
        """Get all comment marker children of this node.

        Returns:
            List of child nodes that are comment markers
        """
        return [child for child in self.children if child.is_comment_marker()]


class ConfluenceTreeParser:
    """Parse Confluence storage format HTML to tree structure."""

    # Confluence XML namespaces
    NAMESPACES = {
        'ac': 'http://www.atlassian.com/schema/confluence/4/ac/',
        'ri': 'http://www.atlassian.com/schema/confluence/4/ri/',
    }

    def parse(self, html: str) -> TreeNode:
        """Parse HTML string to TreeNode structure.

        Args:
            html: Confluence storage format HTML

        Returns:
            Root TreeNode containing the parsed tree
        """
        # Add namespace declarations to the root element
        # This is needed because Confluence HTML fragments use namespace prefixes
        # but don't include xmlns declarations
        namespace_decls = ' '.join(
            f'xmlns:{prefix}="{uri}"' for prefix, uri in self.NAMESPACES.items()
        )
        wrapped = f'<root {namespace_decls}>{html}</root>'

        # Register namespaces for parsing
        for prefix, uri in self.NAMESPACES.items():
            ET.register_namespace(prefix, uri)

        try:
            root = ET.fromstring(wrapped)
            return self._parse_element(root)
        except ET.ParseError as e:
            logger.error(f'Failed to parse HTML: {e}')
            logger.debug(f'HTML content: {html[:500]}...')
            raise

    def _parse_element(self, elem: ET.Element) -> TreeNode:
        """Recursively parse XML element to TreeNode.

        Args:
            elem: XML element to parse

        Returns:
            TreeNode representation of the element
        """
        node = TreeNode(
            tag=elem.tag,
            text=elem.text or '',
            tail=elem.tail or '',
            attrs=dict(elem.attrib),
            children=[],
            element=elem,
        )

        # Parse children
        for child in elem:
            node.children.append(self._parse_element(child))

        return node


class TreeMatcher:
    """Match nodes between old and new trees."""

    def __init__(self, old_tree: TreeNode, new_tree: TreeNode):
        """Initialize matcher.

        Args:
            old_tree: Tree from current Confluence page
            new_tree: Tree from converted markdown
        """
        self.old_tree = old_tree
        self.new_tree = new_tree
        self.matches: dict[int, TreeNode] = {}  # id(old_node) -> new_node mapping

    def match(self) -> dict[int, TreeNode]:
        """Find matching nodes between trees.

        Returns:
            Dictionary mapping old node IDs to their matches in new tree
        """
        # Start by matching children of the root nodes
        # (The root nodes are wrappers we added, so match their children)
        self._match_children(self.old_tree.children, self.new_tree.children)
        logger.info(f'Matched {len(self.matches)} nodes between trees')
        return self.matches

    def _match_recursive(
        self, old_node: TreeNode, new_node: TreeNode
    ) -> bool:
        """Recursively match nodes using multiple strategies.

        Args:
            old_node: Node from old tree
            new_node: Node from new tree

        Returns:
            True if nodes matched
        """
        # Skip comment markers - they're what we're transferring
        if old_node.is_comment_marker():
            return False

        # Strategy 1: Exact match (tag + text signature)
        if old_node.tag == new_node.tag:
            old_text = old_node.get_text_signature()
            new_text = new_node.get_text_signature()

            if old_text == new_text:
                self.matches[id(old_node)] = new_node
                self._match_children(old_node.children, new_node.children)
                return True

            # Strategy 2: Partial match (tag + similar text)
            similarity = self._text_similarity(old_text, new_text)
            if similarity > 0.8:  # 80% similar
                logger.debug(
                    f'Partial match: {old_node.tag} ({similarity:.2f} similarity)'
                )
                self.matches[id(old_node)] = new_node
                self._match_children(old_node.children, new_node.children)
                return True

        return False

    def _match_children(
        self, old_children: list[TreeNode], new_children: list[TreeNode]
    ):
        """Match child nodes.

        Args:
            old_children: Children from old node
            new_children: Children from new node
        """
        # Filter out comment markers from old children
        old_content = [c for c in old_children if not c.is_comment_marker()]

        # Try to match each old child with new children
        for old_child in old_content:
            for new_child in new_children:
                if self._match_recursive(old_child, new_child):
                    break

    def _text_similarity(self, text1: str, text2: str) -> float:
        """Calculate text similarity ratio.

        Args:
            text1: First text
            text2: Second text

        Returns:
            Similarity ratio between 0.0 and 1.0
        """
        if not text1 or not text2:
            return 0.0
        return SequenceMatcher(None, text1, text2).ratio()


class CommentMarkerTransfer:
    """Transfer comment markers from old tree to new tree."""

    def transfer(
        self, matches: dict[int, TreeNode], new_tree: TreeNode, old_tree: TreeNode
    ) -> TreeNode:
        """Transfer comment markers based on node matches.

        Args:
            matches: Mapping of old node IDs to new nodes
            new_tree: The new tree to modify
            old_tree: The old tree to get markers from

        Returns:
            Modified new tree with transferred markers
        """
        transferred_count = 0

        # Build reverse lookup: id -> old_node
        def get_all_nodes(node: TreeNode) -> list[TreeNode]:
            nodes = [node]
            for child in node.children:
                nodes.extend(get_all_nodes(child))
            return nodes

        old_nodes_by_id = {id(n): n for n in get_all_nodes(old_tree)}

        for old_id, new_node in matches.items():
            old_node = old_nodes_by_id.get(old_id)
            if not old_node:
                continue
            # Get comment markers from old node
            markers = old_node.get_comment_markers()

            if markers:
                logger.debug(
                    f'Transferring {len(markers)} markers from {old_node.tag}'
                )
                for marker in markers:
                    self._transfer_marker(old_node, new_node, marker)
                    transferred_count += 1

        logger.info(f'Transferred {transferred_count} comment markers')
        return new_tree

    def _transfer_marker(
        self, old_node: TreeNode, new_node: TreeNode, marker: TreeNode
    ):
        """Transfer a specific marker to new node.

        Args:
            old_node: Original node containing the marker
            new_node: New node to receive the marker
            marker: The comment marker node to transfer
        """
        # Clone the marker node
        new_marker = TreeNode(
            tag=marker.tag,
            text=marker.text,
            tail=marker.tail,
            attrs=marker.attrs.copy(),
            children=[],
            element=None,
        )

        # Find position to insert marker
        # For simplicity, we'll try to match the text content
        marker_text = marker.text.strip()

        if not marker_text:
            logger.warning('Empty comment marker text, skipping')
            return

        # Try to insert marker at the right position
        self._insert_marker_by_text(new_node, new_marker, marker_text)

    def _insert_marker_by_text(
        self, node: TreeNode, marker: TreeNode, marker_text: str
    ):
        """Insert marker by finding matching text in node.

        Args:
            node: Node to insert marker into
            marker: Marker node to insert
            marker_text: Text content that should be wrapped by marker
        """
        # Check if marker text appears in node's direct text
        if marker_text in node.text:
            # Split the text and insert marker
            idx = node.text.index(marker_text)
            before = node.text[:idx]
            after = node.text[idx + len(marker_text) :]

            # Reconstruct: before + marker + after
            node.text = before
            marker.tail = after

            # Insert marker as first child
            node.children.insert(0, marker)
            logger.debug(f'Inserted marker in {node.tag} direct text')
            return

        # Check children for matching text
        for child in node.children:
            if not child.is_comment_marker() and marker_text in child.get_text_signature():
                # Recursively insert in child
                self._insert_marker_by_text(child, marker, marker_text)
                return

        logger.warning(
            f'Could not find position for marker text: "{marker_text[:50]}..."'
        )


class ConfluenceTreeSerializer:
    """Serialize TreeNode back to Confluence storage format."""

    def serialize(self, tree: TreeNode) -> str:
        """Convert TreeNode tree back to HTML string.

        Args:
            tree: Root TreeNode to serialize

        Returns:
            Confluence storage format HTML string
        """
        root = self._build_element(tree)

        # Convert to string
        html = ET.tostring(root, encoding='unicode', method='xml')

        # Remove wrapper - handle both with and without namespace declarations
        # Pattern: <root ...>...</root>
        import re
        html = re.sub(r'^<root[^>]*>', '', html)
        html = re.sub(r'</root>$', '', html)

        return html

    def _build_element(self, node: TreeNode) -> ET.Element:
        """Recursively build XML element from TreeNode.

        Args:
            node: TreeNode to convert

        Returns:
            XML Element
        """
        elem = ET.Element(node.tag, node.attrs)
        elem.text = node.text if node.text else None
        elem.tail = node.tail if node.tail else None

        for child in node.children:
            child_elem = self._build_element(child)
            elem.append(child_elem)

        return elem


class CommentPreserver:
    """Preserve inline comments when updating Confluence pages."""

    def __init__(self):
        """Initialize comment preserver."""
        self.parser = ConfluenceTreeParser()
        self.serializer = ConfluenceTreeSerializer()

    def preserve_comments(self, old_html: str, new_html: str) -> str:
        """Preserve comment markers from old HTML in new HTML.

        Args:
            old_html: Current page HTML with comment markers
            new_html: New HTML from markdown conversion

        Returns:
            New HTML with preserved comment markers
        """
        logger.info('Starting comment preservation')
        logger.debug(f'Old HTML length: {len(old_html)}')
        logger.debug(f'New HTML length: {len(new_html)}')

        try:
            # Parse both HTMLs
            logger.debug('Parsing old HTML...')
            old_tree = self.parser.parse(old_html)
            logger.debug('Parsing new HTML...')
            new_tree = self.parser.parse(new_html)

            # Match nodes
            logger.debug('Matching nodes...')
            matcher = TreeMatcher(old_tree, new_tree)
            matches = matcher.match()
            logger.info(f'Found {len(matches)} matching nodes')

            # Transfer markers
            logger.debug('Transferring markers...')
            transfer = CommentMarkerTransfer()
            modified_tree = transfer.transfer(matches, new_tree, old_tree)

            # Serialize back
            logger.debug('Serializing result...')
            result = self.serializer.serialize(modified_tree)
            logger.debug(f'Result HTML length: {len(result)}')

            logger.info('Comment preservation completed')
            return result

        except Exception as e:
            logger.error(f'Comment preservation failed: {e}')
            logger.warning('Falling back to new HTML without comment preservation')
            import traceback
            logger.debug(traceback.format_exc())
            return new_html
