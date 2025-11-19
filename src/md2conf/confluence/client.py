"""Confluence API client for md2conf.

This module provides async HTTP client for Confluence REST API operations.
Supports creating, reading, and updating pages.
"""

import logging
from typing import Any, NotRequired, TypedDict

import httpx

logger = logging.getLogger(__name__)


# Confluence API Request TypedDicts


class ConfluenceSpaceDict(TypedDict):
    """Confluence space reference."""

    key: str


class ConfluenceBodyDict(TypedDict):
    """Confluence page body content."""

    storage: dict[str, str]  # {"value": html, "representation": "storage"}


class ConfluenceVersionDict(TypedDict):
    """Confluence page version."""

    number: int
    message: NotRequired[str]


class ConfluenceAncestorDict(TypedDict):
    """Confluence page ancestor (parent)."""

    id: str


# Confluence API Response TypedDicts


class ConfluencePageResponseDict(TypedDict):
    """Confluence page response."""

    id: str
    type: str
    title: str
    version: ConfluenceVersionDict
    body: NotRequired[ConfluenceBodyDict]
    _links: NotRequired[dict[str, Any]]


class ConfluenceCommentDict(TypedDict):
    """Confluence comment object."""

    id: str
    title: str
    body: ConfluenceBodyDict


class ConfluenceCommentsResponseDict(TypedDict):
    """Confluence comments response."""

    results: list[ConfluenceCommentDict]
    size: int


class ConfluenceClient:
    """Async HTTP client for Confluence REST API."""

    def __init__(self, client: httpx.AsyncClient, base_url: str):
        """Initialize Confluence client.

        Args:
            client: Authenticated httpx AsyncClient (with OAuth 1.0)
            base_url: Confluence base URL (e.g., https://conf.cian.tech)
        """
        self.client = client
        self.base_url = base_url.rstrip("/")
        self.api_url = f"{self.base_url}/rest/api"

    async def create_page(
        self,
        space_key: str,
        title: str,
        body: str,
        parent_id: str | None = None,
    ) -> ConfluencePageResponseDict:
        """Create a new Confluence page.

        Args:
            space_key: Space key (e.g., "TEST")
            title: Page title
            body: Page content in Confluence storage format (XHTML)
            parent_id: Optional parent page ID

        Returns:
            Created page data including ID and version

        Raises:
            httpx.HTTPError: If request fails
        """
        payload: dict[str, Any] = {
            "type": "page",
            "title": title,
            "space": {"key": space_key},
            "body": {
                "storage": {
                    "value": body,
                    "representation": "storage",
                }
            },
        }

        if parent_id:
            payload["ancestors"] = [{"id": parent_id}]

        logger.info(f'Creating page "{title}" in space {space_key}')
        logger.debug(f"Payload: {payload}")
        response = await self.client.post(
            f"{self.api_url}/content",
            json=payload,
            headers={
                "Content-Type": "application/json",
                "Accept": "application/json",
                "User-Agent": "x",
            },
        )
        if response.status_code >= 400:
            logger.error(f"Error response: {response.text}")
        response.raise_for_status()

        data: ConfluencePageResponseDict = response.json()
        logger.info(f"Created page with ID: {data['id']}")
        return data

    async def get_page(
        self, page_id: str, expand: list[str] | None = None
    ) -> ConfluencePageResponseDict:
        """Get page content and metadata.

        Args:
            page_id: Page ID
            expand: Optional list of fields to expand (e.g., ["body.storage", "version"])

        Returns:
            Page data including title, version, and expanded fields

        Raises:
            httpx.HTTPError: If request fails
        """
        params = {}
        if expand:
            params["expand"] = ",".join(expand)

        logger.info(f"Getting page {page_id}")
        response = await self.client.get(
            f"{self.api_url}/content/{page_id}", params=params
        )
        response.raise_for_status()

        data: ConfluencePageResponseDict = response.json()
        return data

    async def update_page(
        self,
        page_id: str,
        title: str,
        body: str,
        version: int,
        version_message: str | None = None,
    ) -> ConfluencePageResponseDict:
        """Update an existing Confluence page.

        Args:
            page_id: Page ID to update
            title: New page title
            body: New page content in Confluence storage format (XHTML)
            version: Current version number (will be incremented)
            version_message: Optional version comment

        Returns:
            Updated page data including new version

        Raises:
            httpx.HTTPError: If request fails
        """
        payload: dict[str, Any] = {
            "type": "page",
            "title": title,
            "body": {
                "storage": {
                    "value": body,
                    "representation": "storage",
                }
            },
            "version": {"number": version + 1},
        }

        if version_message:
            payload["version"]["message"] = version_message

        logger.info(f"Updating page {page_id} from version {version} to {version + 1}")
        logger.debug(f"Update payload: {payload}")
        response = await self.client.put(
            f"{self.api_url}/content/{page_id}",
            json=payload,
            headers={"Content-Type": "application/json"},
        )
        if response.status_code >= 400:
            logger.error(f"Update error response: {response.text}")
        response.raise_for_status()

        data: ConfluencePageResponseDict = response.json()
        logger.info(f"Updated page {page_id} to version {data['version']['number']}")
        return data

    async def get_comments(self, page_id: str) -> ConfluenceCommentsResponseDict:
        """Get all comments on a page.

        Args:
            page_id: Page ID

        Returns:
            Comments data including count and comment details

        Raises:
            httpx.HTTPError: If request fails
        """
        logger.info(f"Getting comments for page {page_id}")
        response = await self.client.get(
            f"{self.api_url}/content/{page_id}/child/comment",
            params={"expand": "body.storage"},
        )
        response.raise_for_status()

        data: ConfluenceCommentsResponseDict = response.json()
        logger.info(f"Found {data['size']} comments on page {page_id}")
        return data

    async def get_page_url(self, page_id: str) -> str:
        """Get the web URL for a page.

        Args:
            page_id: Page ID

        Returns:
            Full web URL to the page

        Raises:
            httpx.HTTPError: If request fails
        """
        page_data = await self.get_page(page_id)
        # Try to get from _links if available, otherwise construct
        if "_links" in page_data and "webui" in page_data["_links"]:
            return f"{self.base_url}{page_data['_links']['webui']}"
        return f"{self.base_url}/pages/viewpage.action?pageId={page_id}"
