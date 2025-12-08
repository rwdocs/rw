"""Application keys for type-safe app configuration access."""

from aiohttp import web

from docstage.core.cache import FileCache
from docstage.core.navigation import NavigationBuilder
from docstage.core.renderer import PageRenderer

renderer_key = web.AppKey("renderer", PageRenderer)
navigation_key = web.AppKey("navigation", NavigationBuilder)
cache_key = web.AppKey("cache", FileCache)
verbose_key = web.AppKey("verbose", bool)
