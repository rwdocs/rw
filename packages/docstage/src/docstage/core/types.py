"""Core type definitions."""

from typing import NewType

# URL path for routing (e.g., "/guide", "/domain/page")
# Distinct from filesystem Path to catch type mismatches
URLPath = NewType("URLPath", str)
