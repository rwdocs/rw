"""Application keys for type-safe app configuration access."""

from aiohttp import web
from docstage_core import PageRenderer, SiteLoader

renderer_key = web.AppKey("renderer", PageRenderer)
site_loader_key = web.AppKey("site_loader", SiteLoader)
verbose_key = web.AppKey("verbose", bool)
live_reload_enabled_key = web.AppKey("live_reload_enabled", bool)
