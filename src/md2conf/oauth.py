"""OAuth 1.0 authentication for Confluence.

This module provides OAuth 1.0 RSA-SHA1 authentication for Confluence.
Adapted from adrflow project.
"""

from pathlib import Path

import httpx
from authlib.integrations.httpx_client import OAuth1Auth
from cryptography.hazmat.backends import default_backend

# OAuth 1.0 endpoints for Confluence
CONFLUENCE_ENDPOINT = {
    'request_token_url': 'https://conf.cian.tech/plugins/servlet/oauth/request-token',
    'authorize_url': 'https://conf.cian.tech/plugins/servlet/oauth/authorize',
    'access_token_url': 'https://conf.cian.tech/plugins/servlet/oauth/access-token',
}

CONSUMER_KEY = 'md2conf'
CALLBACK_URL = 'http://localhost:8080/callback'


def read_private_key(path: str | Path) -> bytes:
    """Read RSA private key from PEM file.

    Args:
        path: Path to PEM-encoded private key file

    Returns:
        Private key bytes

    Raises:
        FileNotFoundError: If key file doesn't exist
        ValueError: If key format is invalid
    """
    key_path = Path(path)
    if not key_path.exists():
        raise FileNotFoundError(f'Private key file not found: {path}')

    data = key_path.read_bytes()

    # Parse and validate the key
    try:
        from cryptography.hazmat.primitives.serialization import load_pem_private_key

        load_pem_private_key(data, password=None, backend=default_backend())
    except Exception as e:
        raise ValueError(f'Invalid private key format: {e}') from e

    return data


def create_oauth1_auth(
    consumer_key: str,
    private_key: bytes,
    access_token: str,
    access_secret: str,
) -> OAuth1Auth:
    """Create OAuth 1.0 RSA-SHA1 auth instance.

    Args:
        consumer_key: OAuth consumer key
        private_key: PEM-encoded RSA private key bytes
        access_token: OAuth access token
        access_secret: OAuth access token secret

    Returns:
        OAuth1Auth instance for httpx client
    """
    return OAuth1Auth(
        client_id=consumer_key,
        token=access_token,
        token_secret=access_secret,
        signature_method='RSA-SHA1',
        signature_type='HEADER',
        rsa_key=private_key.decode('utf-8'),
    )


def create_confluence_client(
    access_token: str, access_secret: str, private_key: bytes
) -> httpx.AsyncClient:
    """Create OAuth 1.0 authenticated Confluence client.

    Args:
        access_token: OAuth access token
        access_secret: OAuth access token secret
        private_key: PEM-encoded RSA private key bytes

    Returns:
        Authenticated httpx AsyncClient
    """
    auth = create_oauth1_auth(CONSUMER_KEY, private_key, access_token, access_secret)
    return httpx.AsyncClient(auth=auth)
