"""Application keys for type-safe app configuration access."""

from aiohttp import web

from docstage.core.cache import FileCache
from docstage.core.renderer import PageRenderer
from docstage.core.site import SiteLoader

renderer_key = web.AppKey("renderer", PageRenderer)
site_loader_key = web.AppKey("site_loader", SiteLoader)
cache_key = web.AppKey("cache", FileCache)
verbose_key = web.AppKey("verbose", bool)
live_reload_enabled_key = web.AppKey("live_reload_enabled", bool)
