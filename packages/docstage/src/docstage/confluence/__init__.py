"""Confluence integration for Docstage.

This package provides Confluence REST API client and markdown conversion.
"""

from docstage_core import MarkdownConverter

from .client import ConfluenceClient

__all__ = ['ConfluenceClient', 'MarkdownConverter']
