"""
PPTX to HTML Bridge

A library for converting PowerPoint (.pptx) files to HTML format,
and converting the generated HTML back into .pptx.
"""

from .converter import (
    PPTXToHTMLConverter,
    convert_pptx_to_html,
    convert_pptx_directory,
    main
)
from .html_converter import (
    HTMLToPPTXConverter,
    convert_html_to_pptx,
)

__version__ = "0.4.0"
__all__ = [
    "PPTXToHTMLConverter",
    "convert_pptx_to_html",
    "convert_pptx_directory",
    "HTMLToPPTXConverter",
    "convert_html_to_pptx",
    "main"
]