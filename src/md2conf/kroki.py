"""Kroki diagram rendering client.

Renders PlantUML and other diagrams to images using Kroki service.
"""

import base64
import hashlib
import logging
import zlib

import httpx

logger = logging.getLogger(__name__)


class KrokiClient:
    """Client for rendering diagrams via Kroki service."""

    def __init__(self, server_url: str = "https://kroki.io"):
        """Initialize Kroki client.

        Args:
            server_url: Kroki server URL
        """
        self.server_url = server_url.rstrip("/")

    def _encode_diagram(self, source: str) -> str:
        """Encode diagram source for Kroki URL.

        Uses zlib compression and base64 encoding as expected by Kroki.

        Args:
            source: Diagram source code

        Returns:
            URL-safe encoded string
        """
        compressed = zlib.compress(source.encode("utf-8"), level=9)
        encoded = base64.urlsafe_b64encode(compressed).decode("ascii")
        return encoded

    def get_diagram_hash(self, diagram_type: str, source: str) -> str:
        """Generate a hash for the diagram to use as filename.

        Args:
            diagram_type: Type of diagram (plantuml, mermaid, etc.)
            source: Diagram source code

        Returns:
            SHA256 hash prefix (first 12 characters)
        """
        content = f"{diagram_type}:{source}"
        return hashlib.sha256(content.encode("utf-8")).hexdigest()[:12]

    async def render_diagram(
        self,
        diagram_type: str,
        source: str,
        output_format: str = "png",
    ) -> bytes:
        """Render a diagram to image bytes.

        Args:
            diagram_type: Type of diagram (plantuml, mermaid, etc.)
            source: Diagram source code
            output_format: Output format (png, svg, etc.)

        Returns:
            Image data as bytes

        Raises:
            httpx.HTTPError: If request fails
        """
        encoded = self._encode_diagram(source)
        url = f"{self.server_url}/{diagram_type}/{output_format}/{encoded}"

        logger.info(f"Rendering {diagram_type} diagram via Kroki")
        logger.debug(f"Kroki URL: {url}")

        async with httpx.AsyncClient() as client:
            response = await client.get(url, timeout=30.0)
            if response.status_code >= 400:
                logger.error(f"Kroki error: {response.text}")
            response.raise_for_status()

        logger.info(f"Rendered diagram: {len(response.content)} bytes")
        return response.content
