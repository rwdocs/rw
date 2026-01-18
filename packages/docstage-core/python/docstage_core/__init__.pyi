"""Type stubs for docstage_core.

This module re-exports all types from the compiled docstage_core extension module.
"""

from . import config as config
from .docstage_core import (
    ConvertResult as ConvertResult,
)
from .docstage_core import (
    DiagramInfo as DiagramInfo,
)
from .docstage_core import (
    ExtractResult as ExtractResult,
)
from .docstage_core import (
    HtmlConvertResult as HtmlConvertResult,
)
from .docstage_core import (
    MarkdownConverter as MarkdownConverter,
)
from .docstage_core import (
    PreparedDiagram as PreparedDiagram,
)
from .docstage_core import (
    TocEntry as TocEntry,
)

__all__ = [
    "ConvertResult",
    "DiagramInfo",
    "ExtractResult",
    "HtmlConvertResult",
    "MarkdownConverter",
    "PreparedDiagram",
    "TocEntry",
    "config",
]
