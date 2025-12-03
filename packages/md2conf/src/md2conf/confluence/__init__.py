"""Confluence integration for md2conf.

This package provides Confluence REST API client and markdown conversion.
"""

from md2conf_core import MarkdownConverter

from .client import ConfluenceClient

__all__ = ['ConfluenceClient', 'MarkdownConverter']
